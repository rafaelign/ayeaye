# Internationalization (English + Portuguese) — Design

## Context

Every user-facing string in AyeAye is hardcoded in Brazilian Portuguese today, scattered across `crates/app/src/{project_screen,editor_screen,recording_indicator,main}.rs` (`export_screen.rs`, `processing.rs`, `theme.rs`, `crop_tool.rs`, `text_tool.rs` have no user-facing strings — they're background job logic or pure geometry/pixel manipulation). Before calling the app "v1 stable", it needs an English UI (the wider audience for a public repo won't all read Portuguese), auto-selected from the OS locale with English as the fallback, plus a manual switcher so a user can override the detection.

A full inventory of the current strings — which grounds the Component Changes section below and will be turned into exact catalog field names in the implementation plan — turned up 30 distinct pieces of text across those 4 files: 6 in `project_screen.rs` (heading, subtitle, placeholder text, the two mode buttons, the donation footer link), 1 in `recording_indicator.rs` ("Parar"; "REC" and "frames" are already language-neutral loanwords used as-is in the current Portuguese UI, so they don't change), 15 in `editor_screen.rs` (4 per-tool status-bar hints, the play/pause toggle's 2 states, the 4 tool-selector labels, 4 Select-tool action buttons, and the Blur slider label), and 8 in `main.rs` (the editor header's "Exportar" button and "< Nova gravação" link, three state labels — Recording/Processing/Exporting — the "Saved to: …" label, and two interpolated error messages).

## Goals

- Every one of those 30 strings has both an English and a Portuguese (Brazilian) version, selected by a single `Lang` value threaded through the UI.
- On startup, `Lang` is auto-detected from the OS locale (`LC_ALL`, then `LC_MESSAGES`, then `LANG`, in that priority order — POSIX's own precedence: `LC_ALL` overrides everything, `LC_MESSAGES` governs message-catalog language specifically, `LANG` is the general fallback), defaulting to English whenever detection doesn't clearly indicate Portuguese (including all three being unset).
- A two-way toggle ("EN" / "PT-BR") in a slim bar at the top of the main window lets the user override the detected language for the rest of the session, visible from every screen the main window shows (Project, Editor, Processing, Exporting, Done).
- No behavior changes beyond text — this is a translation/plumbing pass, not a UX redesign.

## Non-goals (explicitly out of scope for this pass)

- Persisting a manually-chosen language across restarts. Confirmed with the user: no config file exists anywhere in the app today (no saved project, no saved preferences), and adding one is a bigger decision than this pass should bundle in. Every launch re-detects from the OS; a manual override lasts only the current session.
- Translating the two README files — already separately maintained as `README.md` (English) and `README.pt-BR.md` (Portuguese); out of scope here.
- Translating packaging metadata (`packaging/linux/*.desktop`, `*.metainfo.xml`) — a packaging concern, deferred the same way the Wayland work deferred `.deb`/AppImage packaging itself.
- Translating the native OS file-save dialog (`rfd::FileDialog`) — its chrome is controlled by the OS/toolkit, not us. The one string we do control there, the "GIF" filter label, is a format name and stays as `"GIF"` in both languages, same as the `"REC"`/`"frames"`/`"FPS"`/`"ms"` loanwords already used as-is in the current UI.
- A third+ language, or any translator-facing file format (e.g. `.po`/`.ftl`) — see Alternatives Considered for why a hand-rolled catalog was chosen over an i18n crate that would set that up.
- Adding a language toggle to the two transient viewports (the selection overlay, the recording indicator) — confirmed with the user: the toggle only needs to live on the main window. The indicator is a tiny 240×48 always-on-top strip with no room for it, and the overlay exists only for the few seconds of a drag gesture; neither is a place a user would reach for a language switch, and adding one there would be visual noise for no real benefit.

## Architecture

A new `crates/app/src/strings.rs` module owns two things: the `Lang` enum and the `Strings` catalog struct.

```
Lang::detect()  →  Lang (En | PtBr)
                       │
                       ▼
                Lang::strings()  →  &'static Strings
                       │
     threaded as a `&Strings` parameter into every
     `show()`/render function that currently hardcodes text
```

`App` (in `main.rs`) owns a `lang: Lang` field, initialized via `Lang::detect()` in `App::new`. The top-bar toggle mutates `self.lang` directly when clicked; every render call downstream reads `self.lang.strings()` once per frame and passes the resulting `&'static Strings` down. This mirrors the existing `logo: egui::TextureHandle` field/parameter-threading pattern already used for the app icon — no new plumbing idiom, just one more read-only value passed alongside it.

`Strings` is a plain struct of `&'static str` fields for fixed text, plus a handful of associated functions for the 4 strings that interpolate values whose word order or grammar could differ between languages (`"Frame {X} de {N}"` / `"Frame {X} of {N}"`, `"Salvo em: {path}"` / `"Saved to: {path}"`, and the two error messages). Two `const` values, `EN` and `PT_BR`, fully populate the struct once each; `Lang::strings()` just returns a reference to one or the other. Because every field is required (no `Option`, no fallback-to-English-if-missing), the compiler itself guarantees both languages stay in sync — a new string added to one and forgotten in the other is a compile error, not a runtime blank.

## Component changes

### `crates/app/src/strings.rs` (new)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    PtBr,
}

pub struct Strings {
    pub record_screen_heading: &'static str,
    pub record_screen_subtitle: &'static str,
    // … one field per fixed string identified in the Context inventory …
}

impl Strings {
    pub fn frame_x_of_n(current: usize, total: usize) -> String { /* per-language format */ }
    // … the other 3 interpolated strings, same shape …
}

fn lang_from_env(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Lang {
    // First of the three that's set wins, matching POSIX's own priority
    // order (LC_ALL overrides LC_MESSAGES overrides LANG); a value
    // starting with "pt" (case-insensitive) is Portuguese, anything else
    // — including all three unset — is English.
}

impl Lang {
    pub fn detect() -> Self {
        lang_from_env(
            std::env::var("LC_ALL").ok().as_deref(),
            std::env::var("LC_MESSAGES").ok().as_deref(),
            std::env::var("LANG").ok().as_deref(),
        )
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::PtBr => &PT_BR,
        }
    }
}

const EN: Strings = Strings { /* … */ };
const PT_BR: Strings = Strings { /* … */ };
```

This is the same `*_from_env` + thin public wrapper shape `capture::session_type()`/`session_type_from_env` already established — `lang_from_env` is a pure function taking plain `Option<&str>` values, so it's unit-testable without touching real process environment variables (and without the cross-test interference risk that comes from mutating real env vars, since Rust runs tests in parallel within a binary).

### `crates/app/src/main.rs`

- `App` gains a `lang: Lang` field, set via `Lang::detect()` in `App::new`.
- A new top bar, rendered once per frame before the existing `CentralPanel`, via `egui::Panel::top("app_top_bar").exact_size(28.0)` (same mechanism the editor screen already uses for its own bottom bars) — containing just the two-button language toggle, right-aligned. Panels stack in call order (established during the editor toolbar work), so this is a one-line addition ahead of the existing `CentralPanel::default().show(...)` call, not a restructuring of it.
- `show_editing_body` and every `AppState` match arm that currently calls `ui.label("...")`/`ui.button("...")`/etc. with a literal instead reads from `self.lang.strings()` (fetched once at the top of `ui()`, alongside the existing `let logo = self.logo.clone();` line, and passed down the same way).
- The two error-message `format!(...)` call sites become calls to the corresponding `Strings` associated function.

### `crates/app/src/project_screen.rs`, `editor_screen.rs`, `recording_indicator.rs`

- Each `show()` function gains a `strings: &Strings` parameter (same position/pattern as the existing `logo: &egui::TextureHandle` parameter added for the app icon), and every literal identified in the Context inventory is replaced with the matching field or function call.
- No layout, sizing, or centering logic changes — English and Portuguese strings differ in length (e.g. "Selecionar Área" vs "Select Area", "Excluir frame" vs "Delete frame"), and the project screen's row-width-measured-last-frame centering technique and the editor toolbar's measured-height technique already handle variable-width content by construction (that's the exact problem they were built to solve for the FPS/tool rows) — so switching languages at runtime re-centers itself for free, without new code.

## Testing

- `lang_from_env` gets unit tests: `LANG=pt_BR.UTF-8` → Portuguese; `LANG=en_US.UTF-8` → English; all three unset → English; `LC_ALL` overriding a conflicting `LANG` → `LC_ALL` wins (priority order); a `pt_PT`-style value → still Portuguese (the prefix check isn't Brazil-specific, just "starts with pt").
- `Strings`' interpolated functions (`frame_x_of_n`, etc.) get unit tests confirming each language's exact output string for a representative input.
- No UI test coverage is added or expected — same as every other screen in this app, which has none; the manual checklist in both READMEs gains one line: switch languages via the toggle and confirm every screen's text changes, in both directions.

## Risks / open items

- **Translation quality is mine to get right, not a translator's** — I'm writing both the English and Portuguese strings directly in the implementation plan (there's no separate translator handoff step in this project), so wording should be reviewed once the plan's exact strings are visible, not assumed correct from the field names alone.
- **`egui`'s bundled font** — already confirmed (during the earlier emoji-glyph cleanup) to render plain ASCII and basic Latin-1/accented Portuguese characters fine; English introduces no new glyphs beyond what's already proven to render, so this isn't a new risk, just noted for completeness.

## Alternatives considered

- **`rust-i18n` (YAML catalogs + macros).** Rejected for this pass: scales better if a third language shows up later without touching Rust code, but that's not a stated goal, and it adds macro-driven codegen plus separate catalog files for 30 strings across 2 languages — more moving parts than the problem calls for. A hand-rolled struct keeps the compiler itself as the "missing translation" checker, which a YAML-based catalog doesn't give you for free.
- **`fluent` (Mozilla's ICU-based i18n).** Rejected: built for pluralization, gender agreement, and other grammar-sensitive formatting — none of AyeAye’s 30 strings need any of that (the 4 interpolated ones are simple positional substitutions), so its complexity buys nothing here.
- **Reading locale via a crate (`sys-locale` or similar) instead of raw env vars.** Rejected: this app is Linux-only (X11/Wayland), where `LANG`/`LC_ALL`/`LC_MESSAGES` env vars are the actual, standard mechanism the C locale machinery itself uses — reading them directly, the same way `capture::session_type()` already reads `XDG_SESSION_TYPE`, needs no new dependency and stays consistent with that established pattern in this codebase.
