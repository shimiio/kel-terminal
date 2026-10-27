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
- [ ] F1–F12 and Insert keys
- [x] Commit `examples/` (byte dump and nvim probe tools)

## 2. nvim essentials

- [ ] Replies from the terminal to the program (cursor position `ESC[6n`, device attributes);
      replace the hardcoded `ESC[1;1R` at startup
- [ ] Scroll regions `ESC[r`, insert/delete lines `ESC[L` / `ESC[M`, scroll `ESC[S` / `ESC[T`
- [ ] Save/restore cursor `ESC 7` / `ESC 8` (`esc_dispatch`)
- [ ] Alt+key sends `ESC` + key
- [ ] Application cursor keys (`ESC[?1h`)
- [ ] Bracketed paste (`ESC[?2004h`)

## 3. Config, font, README

- [ ] `config.toml` in `%APPDATA%\kel`: font, size, colors, keys, shell
- [ ] Key to open the config; apply changes without a restart
- [ ] Pick the font by name; Nerd Font icons; fallback font for missing glyphs
- [ ] README with short how-to guides (kept up to date with each feature)

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
