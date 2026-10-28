# Kel

A terminal emulator written in Rust, built to be a daily work tool.

The goal: tabs, and **projects**, saved sets of tabs that remember their name, folder
and startup command (say `nvim .` in one tab and `cargo run` in another), so a whole
workspace opens in one go. See [ROADMAP.md](ROADMAP.md) for what's done and what's next.

Kel runs on Windows for now. Linux and macOS are planned.

## Build and run

Needs Rust 1.88 or newer ([rustup.rs](https://rustup.rs)).

```sh
cargo run            # start Kel
cargo test           # run the tests
cargo build --release   # a fast build: target/release/kel-terminal.exe
```

## Shortcuts

| Keys | What it does |
|---|---|
| Ctrl+Shift+T | New tab |
| Ctrl+Shift+W | Close the tab (typing `exit` does the same) |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+1 … Ctrl+8 | Tab 1 … 8 |
| Ctrl+9 | The last tab |
| Ctrl+= / Ctrl+- | Bigger / smaller font |
| Ctrl+0 | Font size back to the one in the config |
| Ctrl+, | Open the config file |
| Ctrl+Shift+C | Copy the mouse selection (without a selection, the shell gets Ctrl+C) |
| Ctrl+Shift+V | Paste |
| Ctrl+C | Stop the running command |
| Mouse drag | Select text |
| Mouse wheel | Scroll back through earlier output |

## Config

The config file is `%APPDATA%\kel\config.toml`
(e.g. `C:\Users\you\AppData\Roaming\kel\config.toml`).
Kel creates it on the first start, with a comment for every setting.
**Ctrl+,** opens it. Saved changes apply right away, unless the setting says otherwise.

| Setting | Default | What it does |
|---|---|---|
| `font` | `"Consolas"` | Font name, as Windows shows it in Settings > Fonts |
| `font_size` | `18` | Font size in pixels |
| `shell` | `"powershell.exe"` | The program Kel starts. Applies after a restart |
| `[layout]` | `margin = 8`, `border = 1`, `padding = 6` | Spaces around the terminal panel, in pixels |
| `[colors]` | dark gray theme | Text, background, cursor, selection, frame, border and the 16 basic colors |

A setting missing from the file keeps its default, so deleting a line resets it.
To get the original file back with all its comments, delete it and restart Kel.

### Use another font

1. Install the font in Windows (open the `.ttf` file, click **Install**).
2. Press **Ctrl+,** and set its name:
   ```toml
   font = "JetBrainsMono Nerd Font"
   ```
3. Save. The font changes at once.

For the icons that nvim plugins show (file types, git), pick a
[Nerd Font](https://www.nerdfonts.com). In kickstart.nvim, also set
`vim.g.have_nerd_font = true` in `init.lua`.

### Use another shell

```toml
shell = "pwsh.exe"   # PowerShell 7
shell = "cmd.exe"
```

Then restart Kel.

### Change the spaces around the terminal

The terminal is a panel inside the window. Three numbers, in pixels, set the spaces:

```toml
[layout]
margin = 8    # window edge → panel
border = 1    # the panel's line
padding = 6   # panel's line → text
```

All three at `0`: the text fills the window, like a plain terminal.
The colors of the window around the panel and of the line are `frame` and `border`
in `[colors]`.

### Change the colors

Colors are written as `"#rrggbb"`. Set only the ones you want to change, the rest keep
their defaults. For example, the colors of the Tokyo Night theme:

```toml
[colors]
foreground = "#c0caf5"
background = "#1a1b26"
cursor = "#c0caf5"
selection = "#33467c"
palette = [
    "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
    "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#c0caf5",
]
```

The `palette` holds 16 colors: black, red, green, yellow, blue, magenta, cyan, white,
then the bright versions of the same 8. Programs ask for these by number ("red" is 1);
the theme decides how they look. Programs that pick exact colors themselves, like nvim
color schemes, don't use them.

### When something is wrong

- **At start:** Kel uses the default settings and shows the error in red at the top
  of the window, e.g. a missing font or shell, or a typo in the file.
- **After saving:** Kel keeps the settings it had and shows the error in the window
  title, with the line and the reason. Fix the file and save again.

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
| `src/layout.rs` | Where the terminal panel goes in the window |
| `src/theme.rs` | Colors: what programs ask for, and the theme that decides how they look |
| `src/config.rs` | Reads `config.toml` |
| `src/tab.rs` | One tab: a shell, its screen, and the threads that read its output and wait for it to end |
| `src/pty.rs` | Starts the shell inside a pseudo-terminal |
| `src/term/terminal.rs` | Understands the escape codes the shell sends |
| `src/term/grid.rs` | The screen as a grid of cells, plus scrollback |
| `src/renderer.rs` | Turns the grid into pixels |
| `examples/` | Small tools that print the raw bytes a program sends: `cargo run --example probe_nvim` |

Each file starts with a comment that explains it in more detail.
