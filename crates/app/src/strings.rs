#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    PtBr,
}

fn lang_from_env(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Lang {
    let first_set = [lc_all, lc_messages, lang].into_iter().flatten().next();
    match first_set {
        Some(value) if value.to_ascii_lowercase().starts_with("pt") => Lang::PtBr,
        _ => Lang::En,
    }
}

impl Lang {
    /// Detects the session's language from `LC_ALL`, `LC_MESSAGES`, and
    /// `LANG`, in that priority order (POSIX's own precedence). Defaults
    /// to English whenever none of the three is set, or the first one
    /// that is set doesn't clearly indicate Portuguese.
    pub fn detect() -> Self {
        lang_from_env(
            std::env::var("LC_ALL").ok().as_deref(),
            std::env::var("LC_MESSAGES").ok().as_deref(),
            std::env::var("LANG").ok().as_deref(),
        )
    }
}

/// Every fixed piece of UI text in the app, once per supported language.
/// Interpolated strings — where a value is baked into the middle of a
/// sentence and word order can differ by language — are `&self` methods
/// instead of fields (see `frame_x_of_n`, `saved_to`,
/// `recording_stopped_early`, `export_failed`), so they can branch on
/// which language `self` actually is.
pub struct Strings {
    lang: Lang,
    pub donation_link: &'static str,
    pub record_screen_heading: &'static str,
    pub record_screen_subtitle: &'static str,
    pub ready_to_record: &'static str,
    pub full_screen_button: &'static str,
    pub select_area_button: &'static str,
    pub stop_button: &'static str,
    pub hint_crop: &'static str,
    pub hint_blur: &'static str,
    pub hint_text: &'static str,
    pub play_button: &'static str,
    pub pause_button: &'static str,
    pub tool_select: &'static str,
    pub tool_crop: &'static str,
    pub tool_blur: &'static str,
    pub tool_text: &'static str,
    pub duplicate_button: &'static str,
    pub move_left_button: &'static str,
    pub move_right_button: &'static str,
    pub delete_frame_button: &'static str,
    pub intensity_label: &'static str,
    pub export_button: &'static str,
    pub new_recording_link: &'static str,
    pub recording_label: &'static str,
    pub processing_label: &'static str,
    pub exporting_label: &'static str,
}

impl Strings {
    pub fn frame_x_of_n(&self, current: usize, total: usize) -> String {
        match self.lang {
            Lang::En => format!("Frame {current} of {total}"),
            Lang::PtBr => format!("Frame {current} de {total}"),
        }
    }

    pub fn saved_to(&self, path: &std::path::Path) -> String {
        match self.lang {
            Lang::En => format!("Saved to: {}", path.display()),
            Lang::PtBr => format!("Salvo em: {}", path.display()),
        }
    }

    pub fn recording_stopped_early(&self, error: impl std::fmt::Display) -> String {
        match self.lang {
            Lang::En => format!("The recording stopped earlier than expected: {error}"),
            Lang::PtBr => format!("A gravação parou antes do esperado: {error}"),
        }
    }

    pub fn export_failed(&self, error: impl std::fmt::Display) -> String {
        match self.lang {
            Lang::En => format!("Failed to export: {error}"),
            Lang::PtBr => format!("Falha ao exportar: {error}"),
        }
    }
}

const EN: Strings = Strings {
    lang: Lang::En,
    donation_link: "Support the project on Ko-fi",
    record_screen_heading: "Record screen",
    record_screen_subtitle: "Choose the frame rate and record your screen or a selected area.",
    ready_to_record: "Ready to record",
    full_screen_button: "Full Screen",
    select_area_button: "Select Area",
    stop_button: "Stop",
    hint_crop: "Drag over the preview to crop.",
    hint_blur: "Drag over the preview to blur.",
    hint_text: "Click the preview to position the text.",
    play_button: "Play",
    pause_button: "Pause",
    tool_select: "Select",
    tool_crop: "Crop",
    tool_blur: "Blur",
    tool_text: "Text",
    duplicate_button: "Duplicate",
    move_left_button: "< Move",
    move_right_button: "Move >",
    delete_frame_button: "Delete frame",
    intensity_label: "Intensity",
    export_button: "Export",
    new_recording_link: "< New recording",
    recording_label: "Recording...",
    processing_label: "Processing recording...",
    exporting_label: "Exporting...",
};

const PT_BR: Strings = Strings {
    lang: Lang::PtBr,
    donation_link: "Apoie o projeto no Ko-fi",
    record_screen_heading: "Gravar tela",
    record_screen_subtitle: "Escolha a taxa de quadros e grave sua tela ou uma área selecionada.",
    ready_to_record: "Pronto para gravar",
    full_screen_button: "Tela Inteira",
    select_area_button: "Selecionar Área",
    stop_button: "Parar",
    hint_crop: "Arraste sobre o preview para recortar.",
    hint_blur: "Arraste sobre o preview para borrar.",
    hint_text: "Clique no preview para posicionar o texto.",
    play_button: "Reproduzir",
    pause_button: "Pausar",
    tool_select: "Selecionar",
    tool_crop: "Recortar",
    tool_blur: "Blur",
    tool_text: "Texto",
    duplicate_button: "Duplicar",
    move_left_button: "< Mover",
    move_right_button: "Mover >",
    delete_frame_button: "Excluir frame",
    intensity_label: "Intensidade",
    export_button: "Exportar",
    new_recording_link: "< Nova gravação",
    recording_label: "Gravando...",
    processing_label: "Processando gravação...",
    exporting_label: "Exportando...",
};

impl Lang {
    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::PtBr => &PT_BR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_from_env_lang_pt_br_is_portuguese() {
        assert_eq!(lang_from_env(None, None, Some("pt_BR.UTF-8")), Lang::PtBr);
    }

    #[test]
    fn lang_from_env_lang_en_us_is_english() {
        assert_eq!(lang_from_env(None, None, Some("en_US.UTF-8")), Lang::En);
    }

    #[test]
    fn lang_from_env_all_unset_is_english() {
        assert_eq!(lang_from_env(None, None, None), Lang::En);
    }

    #[test]
    fn lang_from_env_lc_all_overrides_a_conflicting_lang() {
        assert_eq!(lang_from_env(Some("en_US.UTF-8"), None, Some("pt_BR.UTF-8")), Lang::En);
    }

    #[test]
    fn lang_from_env_pt_pt_is_still_portuguese() {
        assert_eq!(lang_from_env(None, None, Some("pt_PT.UTF-8")), Lang::PtBr);
    }

    #[test]
    fn frame_x_of_n_in_english() {
        assert_eq!(Lang::En.strings().frame_x_of_n(3, 5), "Frame 3 of 5");
    }

    #[test]
    fn frame_x_of_n_in_portuguese() {
        assert_eq!(Lang::PtBr.strings().frame_x_of_n(3, 5), "Frame 3 de 5");
    }

    #[test]
    fn saved_to_in_english() {
        assert_eq!(Lang::En.strings().saved_to(std::path::Path::new("/tmp/out.gif")), "Saved to: /tmp/out.gif");
    }

    #[test]
    fn saved_to_in_portuguese() {
        assert_eq!(Lang::PtBr.strings().saved_to(std::path::Path::new("/tmp/out.gif")), "Salvo em: /tmp/out.gif");
    }

    #[test]
    fn recording_stopped_early_in_english() {
        assert_eq!(
            Lang::En.strings().recording_stopped_early("disk full"),
            "The recording stopped earlier than expected: disk full"
        );
    }

    #[test]
    fn recording_stopped_early_in_portuguese() {
        assert_eq!(
            Lang::PtBr.strings().recording_stopped_early("disco cheio"),
            "A gravação parou antes do esperado: disco cheio"
        );
    }

    #[test]
    fn export_failed_in_english() {
        assert_eq!(Lang::En.strings().export_failed("timeout"), "Failed to export: timeout");
    }

    #[test]
    fn export_failed_in_portuguese() {
        assert_eq!(Lang::PtBr.strings().export_failed("timeout"), "Falha ao exportar: timeout");
    }
}
