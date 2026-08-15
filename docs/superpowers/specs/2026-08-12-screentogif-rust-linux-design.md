# ScreenToGif em Rust para Linux — Design do MVP

**Data:** 2026-08-12
**Status:** Aprovado

## Contexto

[ScreenToGif](https://github.com/NickeManarin/ScreenToGif) é um gravador de tela e editor de GIF/vídeo em C#/WPF, disponível apenas para Windows. O objetivo deste projeto é uma reescrita em Rust utilizável no Linux.

O ScreenToGif original cobre vários subsistemas relativamente independentes: gravador de tela, gravador de webcam, modo "board" (quadro de desenho), editor de frames com dezenas de efeitos, múltiplos encoders de exportação (GIF, APNG, vídeo, PSD) e gerenciamento de projetos/atualizações. Reescrever tudo de uma vez é grande demais para um primeiro ciclo — este spec cobre apenas o **MVP**: o núcleo gravar → editar → exportar GIF. Os demais subsistemas (webcam, board mode, filtros avançados, outros encoders, projetos salvos em disco) ficam fora de escopo e podem virar specs futuros.

## Escopo do MVP

- Sistema alvo: **Linux com sessão X11** (confirmado como o ambiente do usuário; suporte a Wayland/xdg-desktop-portal fica fora de escopo por ora).
- Fluxo único de sessão: gravar → editar → exportar → fechar. **Sem** salvar/carregar projeto em disco.
- Edição limitada a: excluir frames, reordenar frames, crop (recorte de região aplicado a todos os frames).
- Exportação apenas para **GIF** (sem vídeo, APNG ou PSD).
- Sem webcam, sem board mode, sem overlays de teclado/mouse, sem filtros de imagem.

## Stack de crates

| Necessidade | Crate | Motivo |
|---|---|---|
| GUI | `egui` + `eframe` | Immediate-mode, simples de iterar, janela nativa via wgpu/glow |
| Captura de tela | `xcap` | Já abstrai captura em X11 (usa XShm internamente); evita bindings X11 de baixo nível escritos à mão |
| Hotkeys globais | `global-hotkey` | Funciona em X11 Linux, permite atalhos mesmo sem foco na janela |
| Encoder GIF | `gifski` (como lib) | Qualidade de saída alta, mesmo encoder usado como opção premium no ScreenToGif original |
| Diálogo de arquivo | `rfd` | Diálogo nativo "salvar como" |

Motivo de evitar bindings X11 manuais: a captura de tela via XShm/XGetImage é a parte mais arriscada e trabalhosa do projeto; usar `xcap` reduz esse risco e acelera o MVP.

## Arquitetura

Workspace Cargo com crates separados por responsabilidade:

- **`capture`** — thread de timer que usa `xcap` para capturar a região selecionada em intervalos fixos, empacotando `Frame { image: RgbaImage, timestamp }` e enviando por canal (`mpsc` ou `crossbeam-channel`) para a thread principal.
- **`editor`** — modelo puro em memória: `Vec<Frame>` + operações `delete(index)`, `reorder(from, to)`, `crop(rect)`. Sem dependência de GUI nem de X11 — 100% testável com testes unitários padrão.
- **`encoder`** — wrapper fino sobre `gifski` que recebe `&[Frame]` e produz o arquivo `.gif` final.
- **`app`** (binário) — três telas em `egui`:
  1. Overlay de seleção de região: janela transparente, redimensionável e arrastável.
  2. Editor: tira de miniaturas (drag para reordenar, Delete para remover), preview principal, ferramenta de crop.
  3. Exportação: barra de progresso + diálogo "salvar como" via `rfd`.

## Fluxo de dados

1. App abre → overlay transparente para o usuário posicionar a região de captura.
2. Usuário pressiona hotkey global (ex.: F9) → thread de captura inicia, empurrando frames no canal em intervalos fixos.
3. Usuário pressiona hotkey global de novo → captura para, frames acumulados viram o `Vec<Frame>` do editor.
4. Tela de editor: arrastar miniatura reordena a lista; tecla Delete remove um frame; ferramenta de crop desenha um retângulo sobre o preview e, ao confirmar, aplica o recorte em todos os frames.
5. Botão "Exportar" → `gifski` roda em thread separada relatando progresso → ao concluir, abre diálogo `rfd` de "salvar como" → grava o `.gif` no caminho escolhido.

## Tratamento de erros

- **Falha ao registrar hotkey global** (conflito com outro app já rodando): mostra erro claro na inicialização. O MVP não tenta fallback automático de tecla — o usuário precisa liberar o atalho ou (em versão futura) poder reconfigurá-lo.
- **Falha de captura** (`xcap` retorna erro durante a gravação): aborta a gravação em andamento, mas preserva os frames já capturados até aquele ponto e avisa o usuário, em vez de descartar tudo silenciosamente.
- **Falha no encoder ou de disco** (ex.: disco cheio, caminho inválido): mantém os frames em memória e permite tentar exportar novamente, em vez de perder a sessão de edição.

## Testes

- **`editor`**: testes unitários puros para `delete`, `reorder` e `crop` — não dependem de X11 nem de GUI, rodam em qualquer ambiente de CI.
- **`capture`** e **`encoder`**: dependem de um display X11 real (e, no caso do encoder, de I/O de arquivo), então no MVP ficam como verificação manual. Testes automatizados sob Xvfb são um possível trabalho futuro, fora do escopo deste spec.

## Fora de escopo (para specs futuros)

- Suporte a Wayland (via xdg-desktop-portal + PipeWire).
- Gravação de webcam.
- Modo "board" (quadro de desenho).
- Filtros de imagem, overlays de teclado/mouse, cinemagraph, watermark, etc.
- Outros formatos de exportação: APNG, vídeo (MP4/WebM/AVI via FFmpeg), PSD.
- Salvar/carregar projeto de edição em disco.
- Reconfiguração de atalhos, localização, verificação de atualizações.
