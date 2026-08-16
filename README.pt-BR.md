<p align="center">
  <img src="docs/assets/logo.png" alt="Logo do AyeAye" width="120" />
</p>

<h1 align="center">AyeAye</h1>
<p align="center">Gravador de tela + editor de GIFs para Linux (X11/Wayland), escrito em Rust.</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="Licença MIT"></a>
  <img src="https://img.shields.io/badge/rust-stable-orange.svg" alt="Rust stable">
  <img src="https://img.shields.io/badge/platform-Linux%20(X11%2FWayland)-lightgrey.svg" alt="Linux X11/Wayland">
  <a href="https://ko-fi.com/H2H010PKL5"><img src="https://storage.ko-fi.com/cdn/kofi3.png?v=3" alt="Apoie no Ko-fi" height="20"></a>
</p>

Grava uma região da tela, permite editar os frames (excluir, reordenar, cortar, borrar, anotar com texto) e exporta um GIF. O nome é uma referência ao aye-aye, um lêmure noturno de Madagascar.

> [!NOTE]
> Inspirado no [ScreenToGif](https://github.com/NickeManarin/ScreenToGif) — este projeto é uma reescrita independente em Rust, focada em Linux/X11, sem afiliação com o projeto original.

## Capturas de tela

<p align="center">
  <img src="docs/assets/screenshot_project.png" alt="Tela de gravação" width="420" /><br/>
  <sub>Tela de gravação — escolha o FPS e inicie a gravação da tela inteira ou de uma área.</sub>
</p>
<p align="center">
  <img src="docs/assets/screenshot_editor.png" alt="Tela do editor" width="420" /><br/>
  <sub>Editor — barra de ferramentas acima do preview, com filmstrip e barra de status abaixo.</sub>
</p>

## Requisitos

- Linux com sessão X11 ou Wayland (rode `echo $XDG_SESSION_TYPE` para saber qual).
- Rust estável (`rustup show` para conferir).
- `libpipewire-0.3-dev` e `clang` instalados — necessários para compilar (o suporte a Wayland da `xcap` traz bindings do PipeWire incondicionalmente no Linux, mesmo rodando no X11).

> [!NOTE]
> No Wayland, iniciar uma gravação abre o seletor de compartilhamento de tela do sistema (escolher um monitor, clicar em Compartilhar) — isso é uma barreira de segurança do portal `ScreenCast` do Wayland, não algo que o AyeAye pode pular. O atalho **F9** para parar só funciona no X11; no Wayland, use o botão "Parar" no indicador flutuante de gravação. "Selecionar Área" no Wayland fica limitada ao monitor onde a janela do app está.

## Instalar

Pacotes `.deb` e `.AppImage` prontos ficam anexados a cada
[Release](https://github.com/rafaelign/ayeaye/releases) — veja
`packaging/linux/README.md` para saber como são gerados.

## Build a partir do código-fonte

```bash
cargo build --workspace
```

## Rodar

```bash
cargo run -p app
```

## Fluxo de uso

1. Na tela "Gravar tela", escolha o FPS (8/12/15/20) e clique em **Tela Inteira** ou **Selecionar Área**.
   - **Tela Inteira**: grava o monitor onde a janela do app está.
   - **Selecionar Área**: a tela escurece — arraste um retângulo sobre a região desejada; ao soltar, a gravação começa.
2. Durante a gravação, um indicador flutuante mostra `REC · MM:SS · N frames`. Clique no botão de parar do indicador, ou pressione **F9**, a qualquer momento.
3. A janela principal volta ao primeiro plano com uma tela de carregamento ("Processando gravação...") enquanto as miniaturas são preparadas em segundo plano, e então mostra o editor.
4. No editor: a barra de ferramentas acima do preview traz as ferramentas de edição, o preview fica centralizado abaixo dela, e a filmstrip lista todos os frames (clique para selecionar) acima de uma barra de status. Escolha uma ferramenta na barra:
   - **Selecionar**: Duplicar, mover (`<`/`>`), excluir o frame atual.
   - **Recortar**: arraste sobre o preview para cortar todos os frames.
   - **Blur**: ajuste a intensidade, arraste sobre o preview para borrar uma região em todos os frames.
   - **Texto**: digite o texto, clique no preview para posicioná-lo em todos os frames.
   - **Reproduzir/Pausar** reproduz os frames em loop no preview.
5. Clique em **Exportar**, escolha onde salvar. O editor continua visível (desabilitado) com um indicador de progresso sobreposto; ele libera automaticamente e mostra "Salvo em: ..." quando terminar. "< Nova gravação" descarta a sessão atual e volta à tela inicial.

## Escopo desta versão

Ver `docs/superpowers/specs/2026-08-12-screentogif-rust-linux-design.md` (MVP original), `docs/superpowers/specs/2026-08-13-screentogif-capture-editor-redesign-design.md` (fluxo de captura e editor atuais) e `docs/superpowers/specs/2026-08-15-wayland-capture-support-design.md` (suporte a Wayland) para o design completo. Fora de escopo por enquanto: webcam, modo board, edição de atraso por frame, reordenar por arrastar-e-soltar na filmstrip, escolher uma janela específica para gravar, exportação para vídeo/APNG/PSD, salvar/carregar projeto, um atalho global via portal para o F9 no Wayland.

## Testes automatizados

```bash
cargo test --workspace
```

> [!IMPORTANT]
> `capture` e as partes de janela/viewport do `app` (overlay de seleção, indicador de gravação, esconder/focar a janela principal) não têm testes automatizados — dependem de um display X11 real. Use o checklist manual abaixo para verificá-las; veja também `crates/capture/examples/manual_capture.rs`.

<details>
<summary><strong>Checklist manual end-to-end</strong></summary>

- [ ] Tela Inteira: grava, indicador aparece e conta corretamente, F9 para, editor mostra o resultado com a janela principal em primeiro plano.
- [ ] Selecionar Área: overlay cobre a tela, arrasto mostra o retângulo em tempo real, gravação começa só na área escolhida.
- [ ] No editor: exercite Selecionar (duplicar/mover/excluir), Recortar, Blur, Texto e Prévia, nessa ordem, sobre a mesma gravação.
- [ ] Exportar e abrir o GIF resultante — confirme que ele reflete todas as edições (frame duplicado, corte, blur, texto, ordem).
- [ ] Seletor de idioma: alterne entre EN e PT-BR na barra superior e confirme que o texto de todas as telas muda nas duas direções (tela de gravação, indicador de gravação, barra de ferramentas/status do editor, rótulos de processamento/exportação/concluído).

**No Wayland** (rode numa sessão onde `echo $XDG_SESSION_TYPE` imprime `wayland`):

- [ ] Tela Inteira: grave, confirme que o seletor de compartilhamento do sistema aparece e a gravação só começa depois de escolher um monitor e compartilhar, o indicador aparece e conta corretamente, o botão "Parar" no indicador para a gravação (o F9 não deve fazer nada), o editor mostra o resultado.
- [ ] Selecionar Área: o overlay fica em tela cheia no monitor onde a janela do app está, o arrasto mostra o retângulo em tempo real, os frames exportados/editados cobrem só a região arrastada (não o monitor inteiro).
- [ ] A gravação em cada FPS (8/12/15/20) bate aproximadamente com a contagem de frames esperada pela duração da gravação (com alguma folga — o throttle descarta frames, não garante uma contagem exata).

</details>
