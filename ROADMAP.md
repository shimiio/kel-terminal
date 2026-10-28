# Kel roadmap

Kel is a terminal built to be a daily work tool: tabs, and **projects** that remember
their tabs (name, folder, startup command) so a whole workspace opens in one go.

Kel should run on Windows, Linux and macOS. It's developed on Windows first, so new
code avoids Windows-only things where a cross-platform way is cheap (see the last section).

Stages go in order; items inside a stage can move around.

## Done

- Window, font rendering, shell through ConPTY
- Keys, Ctrl+C, arrows with modifiers, paste (Ctrl+Shift+V), copy a mouse selection (Ctrl+Shift+C)
- Resize, scrollback (mouse wheel, smooth touchpad)
- 16 / 256 / truecolor, bold, italic, underline, reverse video
- Cursor: blinking, show/hide, block / underline / bar
- Alternate screen (vim, less)

## 1. Warm-up

- [x] Font zoom: Ctrl+= / Ctrl+- / Ctrl+0 (cell size from the font instead of constants)
- [x] Glyph cache (stop rasterizing every character on every frame)
- [x] F1–F12 and Insert keys
- [x] Commit `examples/` (byte dump and nvim probe tools)

## 2. nvim essentials

ConPTY sits between nvim and Kel and redraws the screen with simple sequences itself
(checked with `examples/probe_nvim.rs` on Windows 11 26200), so scroll regions,
insert/delete lines, save/restore cursor and application cursor keys never reach Kel.
They are only needed if Kel runs on Linux/macOS someday.

- [x] Replies from the terminal to the program (`ESC[6n`, `ESC[5n`);
      replaces the hardcoded `ESC[1;1R` at startup
- [x] Alt+key sends `ESC` + key
- [x] Bracketed paste (`ESC[?2004h`)

## 3. Config, font, README

- [x] `config.toml` in `%APPDATA%\kel` (font size, shell); errors shown in the window
- [x] Key to open the config (Ctrl+,); apply changes without a restart
- [x] Pick the font by name; Nerd Font icons
- [x] Colors in the config (cells must remember "default color" / "palette color N",
      not the final RGB, so a new theme can recolor text already on screen)
- [x] README with short how-to guides (kept up to date with each feature)
- Keys in the config: later, when there are more shortcuts (tabs)

## 4. Tabs

The look is an experiment: an app frame, the terminal as a panel inside it, the active
tab merging into the panel, other tabs muted. Sizes and colors live in one place
(layout + theme), so trying another look means changing numbers, not code.

- [ ] Layout: the terminal is a panel inside the window (margins, frame color in the theme);
      mouse and grid size measured from the panel
- [ ] `Tab` struct (terminal + PTY + title); `App` keeps a list and the active index
- [ ] PTY events carry the tab id; shell exit closes the tab, not the window
- [ ] Ctrl+Shift+T new, Ctrl+Shift+W close, Ctrl+Tab / Ctrl+Shift+Tab next / previous,
      Ctrl+1…9 go to tab N
- [ ] Tab bar: the active tab merges into the panel, the others stay muted; clickable
- [ ] Activity dot on a background tab with new output
- [ ] Tab/window title from OSC 0/2
- [ ] Ask before closing a tab with a running program

## 5. Projects

- [ ] Shell integration: PowerShell `prompt` reports the current folder (OSC 9;9), Kel remembers it
- [ ] Project file: tabs with name, folder, startup command
- [ ] Open a project: start each tab in its folder and type its command (`nvim .`, `cargo run`)
- [ ] Save the last folder of each tab on close
- [ ] Open by name: `kel <project>`

## 6. nvim polish

- [ ] Mouse reporting (`?1000`, `?1006`) and focus events (`?1004`)
- [ ] Undercurl and underline color (LSP diagnostics)
- [ ] Wide characters (emoji, CJK) take two cells
- [ ] Synchronized output (`?2026`) against flicker
- [ ] `:checkhealth` clean

## 7. Command palette and catalog

- [ ] Command palette (Ctrl+Shift+P): a searchable list window
- [ ] Projects in the palette
- [ ] Catalog: a file of tools (name, description, install command), e.g. lazygit via winget
- [ ] nvim setup script (kickstart deps via winget + clone config), launched from the catalog

## 8. Later

- [ ] Reflow lines on resize
- [ ] Search in scrollback (Ctrl+Shift+F)
- [ ] Clickable links
- [ ] Split panes
- [ ] Window vibrancy
- [ ] License, CI, release `.exe` on GitHub

## Look (ideas, to try)

Not decided, each one gets tried on screen first:
- Project accent color: fills the tab icons, colors the project name
- Pixel-art tab icons (made in Aseprite, PNG), outlines filled with the accent color
- Pixel font for the UI: the Kel logo, the project name
- Sidebar on the left (hover with a short delay, or a key): dims the rest, lists projects
- Display scaling (DPI): font and pixel art must follow Windows' 125% / 150%;
  pixel art only at whole steps (×2, ×3), or its pixels get uneven
- Own title bar instead of the Windows one (then Kel must move, resize, minimize itself)

## Fonts (to decide)

A font that works on any computer without installing it: built into the Kel binary.
Options:
- A. JetBrainsMono Nerd Font as is: 4 files × 2.5 MB, each with the same icons (~10 MB)
- B. JetBrains Mono (4 small files) + Symbols Nerd Font (icons once, any font gets them), ~3–4 MB

Either way:
- [ ] Fallback font for missing glyphs: config font → built-in icons → system emoji/CJK fonts

## Linux / macOS

The base is already cross-platform: winit, softbuffer, fontdue, fontdb, portable-pty, arboard.
What is Windows-only today:
- [ ] Config path `%APPDATA%\kel` (Linux `~/.config/kel`, macOS `~/Library/Application Support/kel`)
- [ ] Ctrl+, opens the config with `explorer.exe` (Linux `xdg-open`, macOS `open`)
- [ ] Default shell `powershell.exe` (Linux/macOS: `$SHELL`)
- [ ] Default font Consolas (solved by a built-in font, see Fonts)
- [ ] Without ConPTY, programs send everything straight to Kel: scroll regions `ESC[r`,
      `ESC[L` / `ESC[M` / `ESC[S` / `ESC[T`, save/restore cursor `ESC 7` / `ESC 8`,
      application cursor keys `ESC[?1h`, device attributes `ESC[c`
- [ ] macOS shortcuts with Cmd instead of Ctrl+Shift
