# Kel roadmap

Kel is a terminal built to be a daily work tool: tabs, and **projects** that remember
their tabs (name, folder, startup command) so a whole workspace opens in one go.

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
- [ ] Fallback font for missing glyphs (emoji, CJK)
- [ ] Colors in the config (cells must remember "default color" / "palette color N",
      not the final RGB, so a new theme can recolor text already on screen)
- [ ] README with short how-to guides (kept up to date with each feature)
- Keys in the config: later, when there are more shortcuts (tabs)

## 4. Tabs

- [ ] `Tab` struct (terminal + PTY + title); `App` keeps a list and the active index
- [ ] PTY events carry the tab id; shell exit closes the tab, not the window
- [ ] Tab bar (drawn by the renderer, clickable)
- [ ] Ctrl+Shift+T new, Ctrl+Shift+W close, Ctrl+Tab next
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
- [ ] Linux/macOS: scroll regions `ESC[r`, `ESC[L` / `ESC[M` / `ESC[S` / `ESC[T`,
      save/restore cursor `ESC 7` / `ESC 8`, application cursor keys `ESC[?1h`
