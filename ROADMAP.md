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

- [x] Layout: the terminal is a panel inside the window (margins, frame color in the theme);
      mouse and grid size measured from the panel
- [x] `Tab` struct (terminal + PTY + title); `App` keeps a list and the active index
- [x] PTY events carry the tab id; shell exit closes the tab, not the window
- [x] Ctrl+Shift+T new, Ctrl+Shift+W close, Ctrl+Tab / Ctrl+Shift+Tab next / previous,
      Ctrl+1…9 go to tab N
- [ ] Later: a config option for Ctrl+Tab order, by position or most recently used (like browsers)
- [x] Move tabs: drag one with the mouse, or Ctrl+Shift+PageUp / PageDown (like browsers);
      a project saves the new order
- [x] Tab bar: the active tab merges into the panel, the others stay muted; clickable
- [x] Own title bar: no Windows title bar, the tab bar takes its place. Kel then does
      what it did: drag to move, double-click to maximize, minimize/maximize/close buttons,
      resize at the edges, the window shadow. Config errors move into Kel's own UI
- [x] Activity dot on a background tab with new output
- [x] Tab/window title from OSC 0/2
- [x] Tab icons: pixel art PNGs in `assets/icons/`, built into the binary, tinted with a color;
      picked by the program in the title (nvim, cargo...); in a plain shell, by the tab's
      name (docker, run/start)
- [ ] Icon by the command that runs (`docker compose up` → docker): PowerShell would send
      the command's text (like VS Code's `ESC]633;E`), its first word picks the icon.
      Order: the command, the title, the tab's name
- [x] Display scaling: the UI and the font follow Windows' scale (100%, 150%...),
      `ui_scale` in the config on top; pixel art only at whole steps (×1, ×2, ×3)

## 5. Projects

- [x] Shell integration: PowerShell and cmd report the current folder (OSC 9;9), Kel remembers it;
      a new tab opens in the current tab's folder
- [ ] Shell integration also marks where a command starts and ends (OSC 133), so Kel knows
      a tab is busy even when the program prints nothing (`cargo run` of a window app):
  - [x] "Busy" mark on the tab (a ring), and "done" when the command ends unseen
        (a green or red dot, by the exit code)
  - [x] Ask before closing a tab (or the window) with a running command: do it again
        within 3 seconds
- [x] Project file: tabs with name, folder, startup command
- [x] Open a project: start each tab in its folder and type its command (`nvim .`, `cargo run`),
      by name: `kel <name>`
- [x] Save the project when the window closes: its tabs, each in its last folder
- [x] `kel` from any shell: the exe is `kel.exe`, and `install.ps1` puts it on the PATH

## 6. nvim polish

- [x] Mouse reporting (`?1000`, `?1002`, `?1003`, `?1006`) and focus events (`?1004`);
      Shift + mouse stays Kel's
- [x] Undercurl and underline color (LSP diagnostics); double, dotted, dashed too
- [x] Wide characters (emoji, CJK) take two cells, counted the way ConPTY does;
      fallback fonts for them (Segoe UI Emoji, Microsoft YaHei, Malgun Gothic)
- [ ] ~~Synchronized output (`?2026`) against flicker~~: measured, nvim doesn't send it
      through ConPTY
- [x] Ctrl+letter as control characters (Ctrl+D, Ctrl+U...), Ctrl+[ \ ] ^ /, Shift+Tab,
      Ctrl+Backspace. Ctrl+Space: measured, nvim on Windows drops it, whatever Kel sends
- [x] `:checkhealth` clean of the terminal's part: Kel sets `TERM_PROGRAM=kel` and `LANG`
      (from Windows' language); the rest it reports is what's installed (stage 7's setup script)

## 7. Command palette and catalog

- [x] Command palette (Ctrl+Shift+P): a searchable list over the terminal, Kel's actions in it
- [x] Projects in the palette: `Open project: kel` (a new window then; in this one since
      the workspaces)
- [x] Install scripts in `setup/`, one per tool, skip what's there: `lazygit.ps1` (winget)
- [x] Catalog: the palette lists `setup/` scripts (built into the exe) and runs one in a new tab
- [x] Buttons for those who don't know the keys: `+` after the tabs (new tab), `×` on a tab
      under the mouse (and a middle click), `⌄` next to `+` (the palette, which shows each
      action's shortcut)
- [ ] `setup/nvim.ps1`: nvim + kickstart deps + clone the config. When the config is ready;
      test it in Windows Sandbox (a clean Windows that's thrown away on close)
- [ ] Languages: Rust and others, with their language servers for nvim

## 8. Every day

What gets in the way when Kel is the only terminal. Kel is used daily from here on, so
what bothers in use can come first.

- [x] Kel's own memory: 213 MB with one tab → 15 MB (measured, release build, JetBrainsMono
      Nerd Font). fontdue read every glyph of the font up front, 4 styles × ~12 600; ab_glyph
      keeps the file and reads a letter when it's first drawn. With an emoji and a Chinese
      letter on screen (their fallback fonts loaded): 453 MB → 46 MB
- [x] Memory of a tab with a full scrollback (10 000 lines, maximized, 171 columns): 65 MB →
      4.4 MB for short lines, 52 → 17 MB for a folder listing. A line kept every cell to the
      window's edge (36 bytes each); now only up to its last non-empty one
- [ ] Later, if needed: a smaller cell (36 → ~16 bytes: a color in 4 bytes, flags in bits),
      and `scrollback = 10000` in the config
- [x] Reflow lines on resize: a narrower window wraps long lines, a wider one joins them back.
      Each row remembers if it wrapped by itself; on resize Kel joins and cuts the lines the
      way ConPTY does (measured: it draws its screen again right after, from the top). A
      shorter window keeps the rows above in the scrollback (they were lost); the 10 000 limit
      counts lines, not rows. A long line copies as one line
- [x] Search in scrollback (Ctrl+Shift+F): in lines, not rows (a word cut at the edge is found),
      any case; the matches keep their place while output comes (rows have lasting numbers).
      ~5 ms per letter in a full scrollback (measured). Scrolled up, the view now stays on its
      text while output comes
- [x] Clickable links: Ctrl+hover underlines, Ctrl+click opens in the browser; found in the
      line's text (joined across the window's edge), the sentence's `.` `,` `)` cut off.
      Only http(s): a `file://` could start a program
- [ ] Later: links a program marks itself (`ESC]8;;url`, OSC 8: needs a link per cell);
      `src/main.rs:42:5` from compiler errors opened in nvim
- [x] Save the tabs as a new project from the palette (the typed text is the name): in the
      folder of the tab on screen, and the window becomes that project. A taken name or one
      Windows can't have (`a/b`, `CON`) gets no line

## 9. Style

Each one gets tried on screen first. Alternates with stage 8: one feature, one look.

- [x] Project accent color: the project's name before the tabs, and the icon of the tab on
      screen, in `color` from the project file (else the theme's `accent`). Tried on screen
      against a line on the tab, a colored block, a colored frame: the name won
- [ ] Maybe: a thin line in the project color on top of the tab on screen (tried on a mockup:
      with the name and the icon, three spots of color; decide after using the name a while)
- [ ] Pixel-art tab icons (made in Aseprite, PNG), outlines filled with the accent color
- [ ] Pixel font for the UI: the Kel logo, the project name (built into the exe, see Fonts)
- [x] Sidebar on the left: `≡` at the tab bar's start (a visible handle: hover alone wouldn't
      say it's there), Ctrl+Shift+B, or the palette. Dims the rest, lists the projects with
      their colors, the last line saves the tabs as a new one. Over the terminal, so nothing
      reflows when it opens. First row `home`: Kel without a project, where you go to the
      projects from
- [ ] Sidebar: open on hover over `≡` (with a delay)? Tried without first; the pixel logo;
      scrolling when there are more projects than fit
- [x] Workspaces in one window: home and projects, switched from the sidebar or the palette.
      Switching hides a set of tabs, nothing stops: a `ping -t` keeps going. Leaving saves
      the project's file; a project's last tab closed goes home. Home has no file: it lives
      while Kel runs, a fresh one after a restart (to decide after using it)
- [x] Sidebar shows what's open and alive: bright dots for the projects open in the window,
      the busy ring where a command runs, the dot where there's new output. Del there (or
      `Close project` in the palette) ends a project's shells: each idle PowerShell is
      ~58 MB (measured). Home's last tab closed closes Kel
- [x] Home as a start screen (like Visual Studio's): the recent projects (by when their file
      was written), ↑↓ Enter or a click opens one; typing puts it away and goes to the shell,
      which starts behind it
- [x] Cards: change in Kel what was only in the project's file. A tab's card (double-click
      it): its name and its command, saved at once
- [x] A project's card (F2 or a right click in the sidebar): its name (the file renamed) and
      color (the theme's colors, or a `#hex` typed, its dot showing it as it's typed). A right
      click on a tab opens its card too
- [ ] The projects' order in the sidebar: drag one, or Alt+↑↓; kept in `projects\order.txt`
- [ ] Window vibrancy

## Later

- [ ] Split panes (tabs and nvim's own windows do for now)
- [ ] License, CI, release `.exe` on GitHub

## Fonts (to decide)

A font that works on any computer without installing it: built into the Kel binary.
Options:
- A. JetBrainsMono Nerd Font as is: 4 files × 2.5 MB, each with the same icons (~10 MB)
- B. JetBrains Mono (4 small files) + Symbols Nerd Font (icons once, any font gets them), ~3–4 MB

Either way:
- [ ] Fallback font for missing glyphs: config font → built-in icons → system emoji/CJK fonts

## Linux / macOS

The base is already cross-platform: winit, softbuffer, ab_glyph, fontdb, portable-pty, arboard.
What is Windows-only today:
- [ ] Config path `%APPDATA%\kel` (Linux `~/.config/kel`, macOS `~/Library/Application Support/kel`)
- [ ] Ctrl+, opens the config with `explorer.exe` (Linux `xdg-open`, macOS `open`)
- [ ] Default shell `powershell.exe` (Linux/macOS: `$SHELL`)
- [ ] Default font Consolas (solved by a built-in font, see Fonts)
- [ ] Without ConPTY, programs send everything straight to Kel: scroll regions `ESC[r`,
      `ESC[L` / `ESC[M` / `ESC[S` / `ESC[T`, save/restore cursor `ESC 7` / `ESC 8`,
      application cursor keys `ESC[?1h`, device attributes `ESC[c`
- [ ] macOS shortcuts with Cmd instead of Ctrl+Shift
