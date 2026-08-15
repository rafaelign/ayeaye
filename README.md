# AyeAye (Rust, Linux/X11)

Grava uma região da tela, permite editar os frames (excluir, reordenar, cortar, borrar, anotar com texto) e exporta um GIF. O nome é uma referência ao aye-aye, um lêmure noturno de Madagascar.

## Requisitos

- Linux com sessão X11 (rode `echo $XDG_SESSION_TYPE` para confirmar — deve imprimir `x11`).
- Rust estável (`rustup show` para conferir).

## Build

    cargo build --workspace

## Rodar

    cargo run -p app

## Fluxo de uso

1. Na tela "Gravar tela", escolha o FPS (8/12/15/20) e clique em **Tela Inteira** ou **Selecionar Área**.
   - **Tela Inteira**: grava o monitor onde a janela do app está.
   - **Selecionar Área**: a tela escurece — arraste um retângulo sobre a região desejada; ao soltar, a gravação começa.
2. Durante a gravação, um indicador flutuante mostra `● REC · MM:SS · N frames`. Clique no botão de parar do indicador, ou pressione **F9**, a qualquer momento.
3. A janela principal volta ao primeiro plano com uma tela de carregamento ("Processando gravação...") enquanto as miniaturas são preparadas em segundo plano, e então mostra o editor.
4. No editor: a miniatura selecionada aparece grande à esquerda; a filmstrip embaixo lista todos os frames (clique para selecionar). No painel à direita, escolha a ferramenta:
   - **Selecionar**: Duplicar, mover (◀/▶), excluir o frame atual.
   - **Recortar**: arraste sobre o preview para cortar todos os frames.
   - **Blur**: ajuste a intensidade, arraste sobre o preview para borrar uma região em todos os frames.
   - **Texto**: digite o texto, clique no preview para posicioná-lo em todos os frames.
   - **▷ Prévia** reproduz os frames em loop no preview.
5. Clique em **Exportar**, escolha onde salvar. O editor continua visível (desabilitado) com um indicador de progresso sobreposto; ele libera automaticamente e mostra "Salvo em: ..." quando terminar. "← Nova gravação" descarta a sessão atual e volta à tela inicial.

## Escopo desta versão

Ver `docs/superpowers/specs/2026-08-12-screentogif-rust-linux-design.md` (MVP original) e `docs/superpowers/specs/2026-08-13-screentogif-capture-editor-redesign-design.md` (fluxo de captura e editor atuais) para o design completo. Fora de escopo por enquanto: Wayland, webcam, modo board, edição de atraso por frame, reordenar por arrastar-e-soltar na filmstrip, escolher uma janela específica para gravar, exportação para vídeo/APNG/PSD, salvar/carregar projeto.

## Testes automatizados

    cargo test --workspace

`capture` e as partes de janela/viewport do `app` (overlay de seleção, indicador de gravação, esconder/focar a janela principal) não têm testes automatizados — dependem de um display X11 real. Use o checklist manual abaixo para verificá-las; veja também `crates/capture/examples/manual_capture.rs`.

## Checklist manual end-to-end

1. Tela Inteira: grava, indicador aparece e conta corretamente, F9 para, editor mostra o resultado com a janela principal em primeiro plano.
2. Selecionar Área: overlay cobre a tela, arrasto mostra o retângulo em tempo real, gravação começa só na área escolhida.
3. No editor: exercite Selecionar (duplicar/mover/excluir), Recortar, Blur, Texto e Prévia, nessa ordem, sobre a mesma gravação.
4. Exportar e abrir o GIF resultante — confirme que ele reflete todas as edições (frame duplicado, corte, blur, texto, ordem).
