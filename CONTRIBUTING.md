# Working on Kel

Kel is a Rust program: [rustup.rs](https://rustup.rs) installs Rust (1.88 or newer).

```sh
cargo run               # start Kel
cargo run -- kel        # open the project "kel"
cargo test              # run the tests
cargo build --release   # a fast build: target/release/kel.exe
.\install.ps1           # build it and install it, as the palette's "Install Kel" does
```

`install.ps1` works while Kel is open: Windows doesn't let a running exe be written
over, so the old one is renamed first, and removed once no Kel runs it.
`.\install.ps1 -NoMenu` installs without Explorer's "Open in Kel".

Before a commit: `cargo fmt`, `cargo clippy --all-targets`, `cargo test`. GitHub
runs the same on every push with the newest Rust, whose clippy knows lints an older
one doesn't.

## How the code is laid out

```text
 keyboard/mouse ──> app ──bytes──> pty (ConPTY) ──> powershell.exe
                                                        │
 window <── renderer <── term::Grid <── term::Terminal <┘ bytes (text + escape codes)
           (pixels)      (cells)        (escape codes → Grid calls)
```

| File | What it does |
|---|---|
| `src/app.rs` | The window, the list of tabs, and all events: keys, mouse, resize |
| `src/keys.rs` | Kel's shortcuts (one table), and the bytes every other key sends to the shell |
| `src/mouse.rs` | The bytes that tell a program what the mouse does, if it asked for the mouse |
| `src/search.rs` | The search in the output: what's typed, the matches, which one is chosen |
| `src/help.rs` | The help's lines: the keys (the shortcuts from the palette's list) and the settings as the config has them now; how far it's scrolled |
| `src/start.rs` | Home's start screen: the recent projects, which one is chosen, where each one is |
| `src/sidebar.rs` | The sidebar of projects: its rows, which one is chosen, where each one is |
| `src/links.rs` | `https://...` in the output: where a link starts and ends |
| `src/open.rs` | Opens a web page, the config or a folder the way the system does: `explorer`, `open`, `xdg-open` |
| `src/palette.rs` | The command palette's state: what's typed, which lines match, which one is chosen |
| `src/setup.rs` | The install scripts from `setup/`, built into the exe, and how a tab runs one |
| `src/layout.rs` | Where the terminal panel goes in the window |
| `src/theme.rs` | Colors: what programs ask for, and the theme that decides how they look |
| `src/config.rs` | Reads `config.toml` |
| `src/glass.rs` | Glass: which colors are see-through, the grain, asking Windows for the blur |
| `src/backdrop.rs` | The start screen's ASCII background: terrain by day, stars at night; or contours, mountains, the sea, wind (`backdrop`) |
| `src/icons.rs` | The tab icons (8 x 8 pixel grids, a pixel 2 x 2 on screen, like the logo) and which tab gets which |
| `src/tab.rs` | One tab: a shell, its screen, and the threads that read its output and wait for it to end |
| `src/project.rs` | Reads and writes a project file: the tabs, their folders and commands |
| `src/pty.rs` | Starts the shell inside a pseudo-terminal |
| `src/shell_integration.rs` | Makes the shell tell Kel its folder, and when a command starts and ends |
| `src/term/terminal.rs` | Understands the escape codes the shell sends |
| `src/term/grid.rs` | The screen as a grid of cells, plus scrollback; on resize, the lines are cut again at the new width |
| `src/renderer.rs` | Turns the grid into pixels |
| `src/font.rs` | Finds a font, and turns its letters into pixels, each one when it's first drawn |
| `src/fallback.rs` | Fonts for the letters the terminal's font lacks (emoji, Chinese), loaded when first needed |
| `examples/` | Small tools that print the raw bytes a program sends: `cargo run --example probe_nvim` |
| `setup/` | Scripts that install Kel itself (`kel.ps1`) and tools (`lazygit.ps1`, with winget). They run from any shell too: `.\setup\lazygit.ps1 -DryRun` only says what it would do |
| `.github/workflows/` | GitHub builds and tests every push, on Windows, Linux and macOS; a tag `v*` makes a draft release with `kel.exe` |

Each file starts with a comment that explains it in more detail.

## Programs in Kel

Kel tells the programs in its tabs where they run: `TERM_PROGRAM=kel` (and
`TERM_PROGRAM_VERSION`), `COLORTERM=truecolor` (Kel draws any color, so delta and
others don't round theirs to the 256-color table), and `LANG` from Windows'
language (`en_US.UTF-8`) unless you set it: nvim and git read their language from it.

Kel knows a tab's folder, and when a command starts and ends, because the shell
tells it before each prompt (`src/shell_integration.rs`). PowerShell and bash (on
Linux and macOS) do all of it, cmd only the folder; other shells nothing yet.

## Sending a change

Anything you send to be added to Kel is under its two licenses, MIT and Apache 2.0
(see the README), unless you say otherwise.
