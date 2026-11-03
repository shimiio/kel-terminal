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
cargo run -- kel     # open the project "kel" (see Projects)
cargo test           # run the tests
cargo build --release   # a fast build: target/release/kel.exe
.\install.ps1        # build it and put it on the PATH: `kel` in any shell
```

## Install tools

In the command palette (Ctrl+Shift+P), type `install`: Enter on a line opens a new tab
that runs that tool's script. The scripts are built into `kel.exe`, so this works
without the repository.

The scripts are in `setup/`, one per tool, and install with winget. They run from any
shell too:

```powershell
.\setup\lazygit.ps1          # lazygit: git in the terminal
.\setup\lazygit.ps1 -DryRun  # only say what it would do
```

Whatever is already there (installed any way: winget, choco, by hand) is skipped, so a
script can run again any time. Afterwards open a new tab: the ones open before don't see
the new program.

## Shortcuts

| Keys | What it does |
|---|---|
| Ctrl+Shift+T | New tab, in the current tab's folder |
| Ctrl+Shift+W | Close the tab (typing `exit` does the same). If a command runs in it (nvim, `cargo run`), press again to close |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+1 … Ctrl+8 | Tab 1 … 8 |
| Ctrl+9 | The last tab |
| Ctrl+Shift+PageUp / PageDown | Move the tab one place left / right. Or drag it with the mouse |
| Ctrl+= / Ctrl+- | Bigger / smaller font |
| Ctrl+0 | Font size back to the one in the config |
| Ctrl+, | Open the config file |
| Ctrl+Shift+P | Command palette: type a few letters (`ntb` finds "New tab"), Up/Down, Enter; Esc or a click closes it |
| Ctrl+Shift+C | Copy the mouse selection (without a selection, the shell gets Ctrl+C). A line too long for the window comes out as one line, not cut where it wrapped |
| Ctrl+Shift+V | Paste |
| Alt+↑ / Alt+↓ in the sidebar | Moves the chosen project up or down; or drag it with the mouse. The order is kept in `projects\order.txt` (new projects go last, by name) |
| F2 in the sidebar | The project's card (or a right click on it, or `Edit project` in the palette): its name, which renames its file, and its color: ←→ through the theme's, or a `#hex` typed, a dot showing it as it's typed |
| Double-click a tab | Its card (or a right click): the tab's name (empty: the program's title) and, in a project, its command, which runs when the project opens. Enter or a click outside keeps it, in the project's file at once; Esc doesn't. Also `Edit tab` in the palette |
| Ctrl+Shift+H | Home, its start screen: position 0, a calm place to start from. In home too: the start screen comes back (unless a command runs there) |
| Ctrl+Shift+B | The sidebar of projects (or `≡` at the tab bar's start). First `home`: Kel without a project. Then the projects, each with its color; Enter or a click goes to one, Del closes an open one, the last line saves the tabs as a new one. Esc or a click outside closes the sidebar |
| Ctrl+Shift+F | Find in the output (not the typed commands: that's PowerShell's Ctrl+R). Type: every match is marked, the newest one chosen. Enter or Up: the one before, Shift+Enter or Down: the next one. Any case: `error` finds `ERROR`, also cut at the window's edge. Esc closes |
| Ctrl+click | Opens the `https://` link under the mouse in the browser (also in nvim). Hold Ctrl and the link is underlined. A link cut at the window's edge is one link; a `.` or `,` after it isn't part of it |
| Ctrl+C | Stop the running command |
| Mouse drag | Select text |
| Mouse wheel | Scroll back through earlier output |
| Shift + mouse | In a program that takes the mouse (nvim, htop, lazygit), the mouse is the program's; with Shift it's Kel's: select text, scroll back |
| `+` after the tabs | New tab |
| `⌄` after the tabs | Command palette, which also lists the shortcuts |
| `×` on a tab under the mouse, or a middle click on a tab | Close that tab (asks first if a command runs in it) |
| Drag the empty top bar | Move the window |
| Double-click the empty top bar | Maximize / restore |
| Drag the window's edge or corner | Resize the window. Long lines wrap again at the new width, the earlier output too |

All other keys go to the program: Ctrl+V and Ctrl+W without Shift too (nvim's block
selection and window commands), and Ctrl with any letter, also on a Russian layout
(Ctrl+В is Ctrl+D).

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
| `ui_scale` | `1` | Size of Kel's own UI (tabs, buttons, spaces), on top of Windows' display scale. `1.25` = a quarter bigger |
| `[layout]` | `margin = 8`, `border = 1`, `padding = 6`, `tab_height = 32`, `tab_width = 180` | Spaces around the terminal panel and the tab sizes, in pixels |
| `[colors]` | dark gray theme | Text, background, cursor, selection, frame, border, `accent` (home's color, none by default: gray) and the 16 basic colors |

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

Kel knows each tab's folder because the shell tells it before each prompt (shell
integration, see `src/shell_integration.rs`). PowerShell and cmd do it on their
own; other shells don't, so their new tabs start in Kel's folder.

PowerShell also tells Kel when a command starts and ends. A tab with a running
command (longer than half a second) has a ring at its right end; one whose command
ended while you were in another tab, a green dot (it worked) or a red one (it failed).
cmd can't say this, so its tabs never show them.

Closing a tab with a running command, or the window while one runs in any tab,
asks first in the tab bar: do it again within 3 seconds to close.

Programs in a tab can tell they run in Kel: it sets `TERM_PROGRAM=kel` (and
`TERM_PROGRAM_VERSION`). It also sets `LANG` from Windows' language, e.g.
`en_US.UTF-8`, unless you set it yourself; programs from Linux (nvim, git) read
their language from it, and nvim's `:checkhealth` asks for it.

### Change the spaces around the terminal

The terminal is a panel inside the window. Three numbers, in pixels, set the spaces:

```toml
[layout]
margin = 8    # window edge → panel
border = 1    # the panel's line
padding = 6   # panel's line → text
tab_height = 32   # the tab bar above the panel
tab_width = 180   # narrower when there are many tabs
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
- **After saving:** Kel keeps the settings it had and shows the error in red at the
  right end of the tab bar, with the line and the reason, e.g.
  `config.toml, line 3: invalid type: string "big", expected f32`.
  Fix the file and save again: the message goes away.

## Projects

A project is a file in `%APPDATA%\kel\projects`, a recipe for a set of tabs. The
file's name is the project's: `projects\kel.toml` opens with `kel kel`
(or `cargo run -- kel`). Each `kel` opens a new window.

From an open Kel: the sidebar (`≡` or Ctrl+Shift+B) or the command palette
(Ctrl+Shift+P, `Open project: kel`). The project comes in this window, and the one
you were in goes out of sight: nothing in it stops, a `ping -t` keeps going, an
unsaved nvim stays unsaved. Choose it again and it's as you left it. `home`, at
the top of the sidebar, is Kel without a project.

The sidebar shows what's open: a bright dot for the projects open in the window, a
faded one for the rest; a ring around it where a command runs ("running"), a small
dot where there's output you haven't seen. Del on an open project closes it: saved,
its shells ended. The palette has `Close project: kel` for the one on screen.

A new home (Kel started without a project, or home again after `Close project`)
shows the start screen over its terminal: the projects used last. ↑↓ and Enter, or a
click, open one; typing puts the screen away and goes to the shell, which has been
starting behind it; Esc only puts it away.

Leaving a project saves its file. Its last tab closed (`exit`): back home. Home's
last tab closed: the project you were in last comes on screen (closing a tab doesn't
end the other projects' shells; home comes back, a new one, when you go there).
Nothing else open: Kel closes. Closing the window saves every open project, and asks
first if a command runs in any of them.

To make one, open the tabs you want, go to the project's folder in one of them, and
type a new name in the palette: `Save the tabs as project: myapp`. The project's
folder is that tab's; the other tabs keep theirs, inside it as `folder = "src"`.
The window then is that project: its name shows before the tabs, and it's saved
when the window closes. A name that's taken has its `Open project` line instead.

`kel` works in any shell after `.\install.ps1`: it builds Kel and copies `kel.exe` to
`~\.cargo\bin`, the folder where cargo puts programs. Installing Rust put that folder
on the PATH, so there's nothing to add by hand. Run it again after changes: it works
while Kel is open (Windows doesn't let a running exe be written over, so the old one
is renamed first, and removed once no Kel runs it).

```toml
folder = 'C:\code\kel-terminal'   # single quotes, so a '\' stays a '\'
color = "#e0af68"                 # optional, see below

[[tab]]
name = "editor"      # shown in the tab bar instead of the program's title
command = "nvim ."   # typed into the shell once it's ready, as if you did

[[tab]]
name = "run"
command = "cargo run"

[[tab]]
folder = "docs"      # inside the project's folder; a full path works too
```

Every line of a `[[tab]]` is optional: `[[tab]]` alone is a shell in the project's
folder. A file without any is one such tab. The running programs aren't saved, they
can't be: opening the project starts them again.

A project's window shows its name before the tabs, so you see which one it is at a
glance. `color` is the project's own accent: the name and the icon of the tab on
screen are in it, and the logo's cursor and the window's icon in the taskbar. Without
`color` the project is gray, like home: Kel has no color of its own, it takes the color
of what you work on (`accent` in `[colors]` gives home and such projects one).

A tab's icon shows what runs in it (nvim, git), as the program's title says. In a
plain shell, the tab's `name` picks it: a tab called `docker` has the whale, `run`
or `start` a ▶. The name itself stays whatever runs.

When the window closes (the × button or Alt+F4), Kel writes the file again with the
tabs open then, so the project opens next time as you left it:

| | Saved? |
|---|---|
| The tabs, left to right (closed ones go, new ones come) | yes |
| Each tab's last folder (`cd src`) | yes, if the shell reports it (PowerShell, cmd) |
| A tab's `name` and `command` from the file | yes |
| Commands you typed (`git log`, `cargo run`) | no: opening a project never runs one by itself |
| The text on the screen | no |

Edit the file while the project isn't open, or Kel writes over your changes.
Comments in it don't stay. Closing the tabs one by one (`exit`) doesn't save:
the file keeps the tabs it had, not none.

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
| `src/start.rs` | Home's start screen: the recent projects, which one is chosen, where each one is |
| `src/sidebar.rs` | The sidebar of projects: its rows, which one is chosen, where each one is |
| `src/links.rs` | `https://...` in the output: where a link starts and ends |
| `src/palette.rs` | The command palette's state: what's typed, which lines match, which one is chosen |
| `src/setup.rs` | The install scripts from `setup/`, built into the exe, and how a tab runs one |
| `src/layout.rs` | Where the terminal panel goes in the window |
| `src/theme.rs` | Colors: what programs ask for, and the theme that decides how they look |
| `src/config.rs` | Reads `config.toml` |
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
| `setup/` | Scripts that install tools (see Install tools) |

Each file starts with a comment that explains it in more detail.
