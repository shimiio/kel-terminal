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
- [x] Tab icons: pixel art (PNGs at first, grids in code now), tinted with a color;
      picked by the program in the title (nvim, cargo...); in a plain shell, by the tab's
      name (docker, run/start)
- [x] More icons: claude (a spark), ssh (a server; also a tab called `vps`), database (a
      cylinder; `db`, `psql`, `redis`, `lazysql`...)
- [x] Icon by the command that runs (`docker compose up` → docker): PowerShell sends the
      command's text (VS Code's `ESC]633;E`), its program picks the icon, else any word.
      Order: the title (nvim's own), the command (once busy: no blink), the tab's name.
      New icons with it: python (a snake), node (a hexagon), rust/cargo (a gear)
- [x] Hints in italics (the built-in JetBrains Mono Italic): the start screen's, the
      palette's keys, the sidebar's and the help's last lines, the card's. They read as
      side notes, apart from names and commands. Questions and errors stay upright
- [x] The command as the tab's name while it runs, when it gave no title of its own (the
      title is still the one from before it): `ping -t ...`, not `powershell`. Not for
      a tab you named. Icons for what a tab is for: backend `{ }`, frontend (a browser's
      window), repo (git)
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
- [x] Help (Ctrl+Shift+/, not F1: programs have it): every key by where it works, every
      setting with its value now; the shortcuts from the palette's list, never out of date
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
      screen, in `color` from the project file (else gray, or the theme's `accent`). Tried on screen
      against a line on the tab, a colored block, a colored frame: the name won
- [ ] Maybe: a thin line in the project color on top of the tab on screen (tried on a mockup:
      with the name and the icon, three spots of color; decide after using the name a while)
- [x] Pixel-art tab icons like the logo: 8 x 8 grids in `icons.rs`, a pixel 2 x 2 on screen,
      so it shows it's pixels; in the window's color. The PNGs and their decoder are gone
- [x] The Kel logo: "kel" and a cursor, in pixels (`logo.rs`). Big on the start screen,
      the cursor blinking with the terminal's; small, still, at the sidebar's top. The cursor
      is the project's color; home has none (the text's): Kel takes the color of what you
      work on
- [x] The window's icon from the logo's pixels (`k▌`, a square of the background's color with
      round corners), its cursor the project's color: in the taskbar too
- [x] The exe's icon (Explorer, Start, a pinned shortcut): `build.rs` makes the `.ico` from
      `logo.rs` at each build (home's gray, 16..256 px) and puts it in `kel.exe` with
      `winresource`; the file's details say "Kel" (Task Manager)
- [x] The start screen's backdrop: ASCII behind the logo and the list, dim, clear around
      them. By day a terrain (smooth noise as ` .:-=+*`), at night (22:00 to 6:00) stars;
      new each time it shows (a seed; `backdrop.rs`)
- [x] Home is position 0: coming back to it (the sidebar, the palette, a project closed)
      shows its start screen again, the projects used last fresh; home chosen while in home
      too. Not while a command runs in its tab: you came back to watch it. Its tab says
      "home", with Kel's mark `k▌`, while the screen is up. Ctrl+Shift+H goes there
- [x] Home neutral in the whole UI: the project's color is the only one (`accent_for`); home
      and projects without one are gray. `accent` in `[colors]` is optional now
- [x] `COLORTERM=truecolor` for the programs in a tab: delta (lazygit's diffs) sent its
      colors rounded to 256, its dark red and green behind lines came out loud
- [x] Soft defaults: the terminal's 16 colors of the same family as `project_colors` (not
      Windows' Campbell), a gray selection (a project's color would look like the search's
      matches), acrylic glass with a light grain (0.1)
- [x] The cursor a whole cell by default (`cursor` in the config: block, underline, bar;
      programs still pick theirs). Where the keys don't go (the sidebar, the palette, the
      search, a card, the help, another window): a hollow box, not blinking
- [x] Kel's own soft colors (`project_colors`, `done`, `failed` in `[colors]`): a project's
      color is picked among 8 of one brightness, not the terminal's loud palette; a
      command's dot and Kel's errors in the soft green and red
- [x] The project's name in the tab bar in pixels, like the logo: a 5 x 7 font drawn in
      `logo.rs` (lowercase, digits, `- _ .`; the logo's k e l are its letters), no font
      file. A name with a letter it hasn't got (Cyrillic...) stays in the UI's font
- [x] Sidebar on the left: `≡` at the tab bar's start (a visible handle: hover alone wouldn't
      say it's there), Ctrl+Shift+B, or the palette. Dims the rest, lists the projects with
      their colors, the last line saves the tabs as a new one. Over the terminal, so nothing
      reflows when it opens. First row `home`: Kel without a project, where you go to the
      projects from
- [x] The sidebar's button: a panel with a column on its left (`◫`), what it opens, in
      the thin lines of `+` and `⌄`, instead of `≡`. Not pixels: those are the tabs' icons
- [x] Sidebar peek: the mouse resting on `≡` 0.4 s opens it, leaving it closes it; a click
      or a key in it keeps it. Passing over `≡` on the way to a tab doesn't open it
- [x] Workspaces in one window: home and projects, switched from the sidebar or the palette.
      Switching hides a set of tabs, nothing stops: a `ping -t` keeps going. Leaving saves
      the project's file; a project's last tab closed goes home. Home has no file: it lives
      while Kel runs, a fresh one after a restart (to decide after using it)
- [x] Sidebar shows what's open and alive: bright dots for the projects open in the window,
      the busy ring where a command runs, the dot where there's new output. Del there (or
      `Close project` in the palette) ends a project's shells: each idle PowerShell is
      ~58 MB (measured). Home's last tab closed: the project you were in last comes on
      screen (home comes back new); with no project open, Kel closes
- [x] Home as a start screen (like Visual Studio's): the recent projects (by when their file
      was written), ↑↓ Enter or a click opens one; typing puts it away and goes to the shell,
      which starts behind it
- [x] Delete a project: Shift+Del in the sidebar or `Delete project` in the palette, on the
      second press; its file and its line in `order.txt` go, its folder stays
- [x] Cards: change in Kel what was only in the project's file. A tab's card (double-click
      it): its name and its command, saved at once
- [x] A project's card (F2 or a right click in the sidebar): its name (the file renamed) and
      color (the theme's colors, or a `#hex` typed, its dot showing it as it's typed). A right
      click on a tab opens its card too
- [x] The projects' order in the sidebar: drag one, or Alt+↑↓; kept in `projects\order.txt`
      (new ones last, by name). The start screen stays by when they were used
- [x] Glass, a setting (`[glass]`, off by default): Windows' Acrylic (blur) or Mica behind
      the background and the frame, the theme's color over it (`opacity`) and a still grain
      (`grain`). A color's top byte says how clear it is, so letters' edges blend right;
      on battery Windows may drop the blur, then the glass is a plain color

## 10. Before the release

- [x] Pasting several lines asks first (a site's command can hide a second line that runs
      on its own; Windows Terminal warns too). Not when the program takes pasted text
      as a whole (`ESC[?2004h`). One line goes in without its line break: it waits
- [x] Sidebar: scrolling when there are more projects than fit (the wheel, ↑↓ past an end,
      a drag past an end); `home` and "Save" stay put; a thin bar shows where you are
- [x] A click doesn't select its one cell: only a drag selects (back on the first cell
      after a drag: that one cell, on purpose)
- [x] Double-click selects the word (a path or an address whole, without a sentence's
      dot after it), triple-click the line (all its rows); dragged after: by words, by lines
- [x] OSC 8 links: text a program marks as a link (`ls --hyperlink`, delta, compilers),
      its address not on the screen, so the tab bar shows it (its middle cut). Ctrl+click:
      a web page in the browser; a file here shown in Explorer, never run; a file on
      another computer not at all (Windows would send your login to look)
- [x] Readability under lazygit / Claude Code: a solid background behind full-screen
      programs (the alternate screen), as nvim paints its own; Claude Code draws in the
      normal screen, it's known by its icon while it runs. The frame stays glass
- [x] The palette's chosen line in the project's color: a tint (tried gray, tints of
      45 and 60, a bar at the left with and without one; the tint alone is calmer, like
      the sidebar's chosen row). Home stays gray
- [x] Underlines: looked over at sizes 12, 15, 22 (single, double, curly, dotted, dashed;
      colored; under wide letters, bold, italic, reverse, a background): all hold. An
      underline color written with `;` (`58;2;r;g;b`) never arrives: ConPTY drops it;
      nvim's `58:2::r:g:b` passes
- [x] Ctrl+Alt+← / → : the previous / next open project (and home), as Zen's workspaces:
      in the sidebar's order, which you set; home first, always there. Ctrl+Alt is AltGr,
      but the arrows type nothing with it
- [x] "Open in Kel" on a folder in Explorer (`kel <folder>`): install.ps1 puts it in the
      classic menu (Windows 11: "Show more options"); a project's folder opens the project.
      In the short Windows 11 menu: only as a package (MSIX), maybe with the release
- [x] Home's ASCII landscape: the terrain with air between its hills (the lows
      empty, the shapes bigger: it was a wall of `=` and `-`). Other pictures tried
      (contours, mountains, the sea, wind) and kept, not shown by default: `backdrop`
      in the config picks one. Maybe later: a random one each time
- Settings review before Linux/macOS, so the config's shape is settled before it has
  to work on three systems. Chosen, in this order:
  - [x] `shell` with its arguments (`"pwsh.exe -NoLogo"`, `"wsl.exe -d Ubuntu"`), the
        system's without it (PowerShell; `$SHELL` on Linux/macOS); new tabs take a change
  - [x] A shell per project (`shell` in its file), and per tab (`shell` in a `[[tab]]`);
        new tabs get the project's
  - [x] `bell`: the taskbar button blinks while Kel is behind (`"taskbar"`), and the
        terminal flashes while it's in front (`"flash"`), or nothing. It was ignored
  - [x] `cursor_blink`
  - [x] `copy_on_select`
  - [x] `scrollback` (10 000 lines by default)
  - [x] `line_height`: frames and blocks reach the taller cell's edges, no gaps; and
        no more thin dark seam between two `█` (the font's soft edge), at any height
  - [x] `start_screen` on / off
  - After the release: `[keys]` (your own shortcuts; Cmd on macOS), themes as files
    (`theme = "tokyo-night"`), the window remembering its size
- [x] More places that read better solid (as under lazygit): every full-screen program
      already; of the normal screen's, `solid_for` in `[glass]`, AI helpers by default
      (claude, codex, gemini, aider, opencode, copilot, crush), more added as they come
- [x] Ready for a release: two licenses (MIT or Apache-2.0); kel.exe needs no DLL a
      clean Windows lacks; it installs itself (the palette: Start menu, PATH, Explorer)
      and uninstalls; GitHub tests every push and drafts a release from a `v*` tag
- [ ] The first release, v0.1.0: Windows. Tried on a clean Windows laptop
- [ ] Linux and macOS, after it, as a preview: CI builds them, so testers need no Rust
      (a Linux in WSL, a friend's Mac). See below

Settled:
- The project's folder in the sidebar is where its tabs start, not where a tab went:
  it's the project's place, like a tab's name (a `cd` doesn't move the project)
- Kel's UI already has its own font (the built-in JetBrains Mono), whatever `font` is;
  monospaced like the rest of Kel
- Moving tabs: Ctrl+Shift+PageUp / PageDown (or a drag). Not Alt+arrows: programs get them

## After the release

- [ ] Sessions that come back: a tab's output from last time, above its new prompt
- [ ] A history per tab: ↑ shows the commands typed in this tab of this project (PowerShell's
      `HistorySavePath` per tab, set by the shell integration). Ghost suggestions from it
      (PSReadLine's `PredictionSource History`, PSReadLine 2.1+)
- [ ] A small local AI (Ollama): reads the tab's output, suggests a command into the input
      line, never runs it. Try first whether a small model is good enough
- [ ] Split panes, like tmux inside Kel (tabs and nvim's own windows do for now)
- [ ] Emoji in color (Segoe UI Emoji's COLR layers; now in the text's color)

## Fonts

- [x] Built into the exe (~3.5 MB): JetBrains Mono (4 styles) + Symbols Nerd Font Mono (the
      icons once, any font gets them). Not a Nerd version of each style (~10 MB)
- [x] Kel's UI always in the built-in font; `font` is the terminal's (default the built-in
      one; one that isn't installed: the built-in one, and a line saying so)
- [x] Fallback for missing glyphs: the terminal's font → built-in icons → Windows'
      emoji/CJK fonts

## Linux / macOS

The base is already cross-platform: winit, softbuffer, ab_glyph, fontdb, portable-pty, arboard.
What is Windows-only today:
- [x] Config path: `%APPDATA%\kel`; Linux and macOS `~/.config/kel` (or `$XDG_CONFIG_HOME/kel`),
      where terminals and nvim keep theirs, not macOS' `~/Library/Application Support`
- [x] Ctrl+, and links open with the system's program (`src/open.rs`: Linux `xdg-open`, macOS `open`)
- [x] Kel compiles on Linux and macOS; CI builds and tests all three
- [x] Default shell `powershell.exe` (Linux/macOS: `$SHELL`)
- [x] Default font Consolas (solved by the built-in JetBrains Mono)
- [ ] The config's comments speak of Windows (Settings > Fonts, `pwsh.exe` examples)
- [ ] Shell integration (the tab's folder, a command's start and end) for bash, zsh, fish
- [ ] Install on Linux (`~/.local/bin`, a `.desktop` file) and macOS (`Kel.app`)
- [ ] Without ConPTY, programs send everything straight to Kel: scroll regions `ESC[r`,
      `ESC[L` / `ESC[M` / `ESC[S` / `ESC[T`, save/restore cursor `ESC 7` / `ESC 8`,
      application cursor keys `ESC[?1h`, device attributes `ESC[c`
- [ ] macOS shortcuts with Cmd instead of Ctrl+Shift
- [ ] macOS Option+letter types `ø`, `å`: Kel now sends Alt+letter by the key's place
      for a non-Latin letter (Alt+М is Alt+V), which would make Option+O `ESC o`.
      Decide: Option as Meta (terminals offer a setting) or as letters
