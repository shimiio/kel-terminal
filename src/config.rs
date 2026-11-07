//! The config file: `config.toml` in Kel's folder (see `dir`).
//!
//! On the first start Kel writes the default file, with a comment for every setting.
//! A setting missing from the file keeps its default value.

use crate::backdrop;
use crate::glass::Glass;
use crate::layout::Spacing;
use crate::term::CursorShape;
use crate::theme::Theme;
use anyhow::Context;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::SystemTime;

/// The file Kel writes on the first start. Its values must match `Config::default()`
/// (a test checks it).
// `r##"..."##`: a raw string, ending at `"##`. The colors hold `"#`, which would
// end a plain `r#"..."#` string early.
const DEFAULT_CONFIG: &str = r##"# Kel config. Lines starting with # are comments.
# Ctrl+, opens this file. Saved changes apply right away, unless a setting says otherwise.
# Delete a setting to go back to its default value.

# The terminal's font. "JetBrains Mono" is built into Kel: it works on any computer.
# Or any installed font, by the name Windows shows in Settings > Fonts.
# Kel's own UI (tabs, menus, the sidebar) is always JetBrains Mono, and the icons
# nvim plugins use (Nerd Font's) are built in too, for any font.
font = "JetBrains Mono"

# Font size in pixels. Ctrl+= / Ctrl+- change it until Kel closes, Ctrl+0 comes back to this.
font_size = 18

# The space between lines: 1 as the font has it, 1.2 a fifth more (0.8 to 2).
line_height = 1

# The program Kel starts in a tab, with its arguments, as you'd type it:
# "pwsh.exe -NoLogo" (PowerShell 7), "cmd.exe", "wsl.exe -d Ubuntu".
# A path with spaces goes in "": '"C:\Program Files\PowerShell\7\pwsh.exe" -NoLogo'
# Without it: the system's (Windows: PowerShell; Linux, macOS: your $SHELL).
# Applies to new tabs.
# shell = "pwsh.exe -NoLogo"

# The size of Kel's own UI (tabs, buttons, the spaces below), on top of Windows'
# display scale (Settings > Display > Scale). e.g. 1.25: a quarter bigger.
# The terminal's text has font_size for that.
ui_scale = 1

# The project's name before the tabs in pixels, like Kel's logo. false: in the font.
pixel_name = false

# The cursor: "block" (the whole cell), "underline" or "bar".
# Programs can pick their own (nvim does); this is for the rest.
cursor = "block"
cursor_blink = true   # false: it stands still (the logo's and the sidebar's too)

# true: what you select with the mouse is copied as you let go, no Ctrl+Shift+C.
copy_on_select = false

# macOS: true makes Option the Alt key: Alt+B and Alt+F a word back and on in
# the shell, nvim's Alt keys. false: it types its letters, as on any Mac (@ and
# { on German and other layouts are Option+letter).
option_as_alt = false

# How many lines of output each tab keeps to scroll back to. Older ones go.
# 10 000 lines take a few MB per tab. A smaller number applies at once.
scrollback = 10000

# The bell, a program's "done" or "come here" (Claude Code can ring it):
# "taskbar": Kel's button in the taskbar blinks while you're in another window,
# until you come back; "flash": that, and the terminal flashes when you're here;
# "none": nothing.
bell = "taskbar"

# Home's start screen: the logo and the projects used last, when you come home.
# false: home is a plain terminal.
start_screen = true

# The start screen's picture behind the logo, new each time it shows:
# "auto" (a terrain by day, stars at night), "terrain", "contours", "mountains",
# "sea", "wind", "stars", or "none". Mountains and the sea have stars at night.
backdrop = "auto"

# Spaces around the terminal, in pixels. The terminal is a panel inside the window:
# margin (window edge → panel), border (the panel's line), padding (line → text).
# All 0: the text fills the window, like a plain terminal.
# border is every line of Kel's: the panel's, the sidebar's, the palette's, the help's.
# 0: none, those stand out by their color.
[layout]
margin = 8
border = 0
padding = 6
tab_height = 32   # the tab bar above the panel
tab_width = 180   # narrower when there are many tabs
# The title bar's buttons: "always", "hover" (while the mouse is on the title bar)
# or "never" (their place goes to the tabs; their keys still work).
sidebar_button = "hover"
tab_buttons = "hover"       # + and ⌄ after the tabs
window_buttons = "hover"    # minimize, maximize, close
corner_radius = 6   # round corners of the panel, the tab, the boxes: 0 square, 8 soft

# Glass: Kel see-through, Windows blurring what's behind it.
# effect: "acrylic" (what's behind, blurred), "mica" (a faint tint of the
# wallpaper, Windows 11), "none" (solid). Applies after a restart.
# Windows turns the blur off by itself at times (e.g. on battery): then the
# glass is a plain color.
[glass]
effect = "acrylic"
opacity = 0.8    # how much of the theme's colors cover the glass: 1 solid, 0 all glass
grain = 0.1      # a fine grain on the glass, like frosted glass: 0 none, 1 strong
solid_under_apps = true   # a solid background under lazygit, htop, Claude Code: they read better
# Full-screen programs get it all. These draw in the normal screen and get it too,
# by the program's name or a word of the title it gives the tab. Add your own.
solid_for = ["claude", "codex", "gemini", "aider", "opencode", "copilot", "crush"]
# solid_color = "#000000" # its color; without it, the theme's background
cells = true   # the gray backgrounds programs pick (nvim's color scheme) are glass too, colored ones (a diff's red, green) stay solid; false: all solid

# Colors, as "#rrggbb". Programs ask for "red" or "default text"; these say how they look.
# Programs that pick exact colors themselves (like nvim color schemes) don't use them.
[colors]
foreground = "#ffffff"
background = "#141416"
cursor = "#a0a0a0"
selection = "#3d3d44"
frame = "#0e0e10"    # the window around the panel
border = "#333333"   # the panel's border line
# accent = "#569cd6" # home's color, and of projects without their own: the tab
                     # icons, the logo's cursor. Without it: none, the text's gray
# Kel's own colors, soft: a project's color is one of these (or any #hex).
project_colors = [
    "#e06c75", "#e5a46b", "#e5c07b", "#98c379", "#6cc4b8", "#7aa2e6", "#b39ae6", "#e690b8",
]
done = "#98c379"     # a command that ended well: the dot on its tab
failed = "#e06c75"   # one that failed; and Kel's own errors
# The terminal's 16 colors, which programs color their text with:
# black, red, green, yellow, blue, magenta, cyan, white,
# then the bright versions of the same 8.
palette = [
    "#5c5c5c", "#ff4d5e", "#6dff5c", "#ffe14d", "#4dabff", "#d47dff", "#2df5dc", "#f0f0f0",
    "#a6a6a6", "#ff7a86", "#9dff91", "#fff08a", "#8cc8ff", "#e6adff", "#7ff9ea", "#ffffff",
]
"##;

/// What the bell does: `bell` in the config.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Bell {
    /// The taskbar button blinks while Kel isn't in front.
    #[default]
    Taskbar,
    /// That, and the terminal flashes while Kel is in front.
    Flash,
    None,
}

impl Bell {
    /// As the config spells it.
    pub fn name(self) -> &'static str {
        match self {
            Bell::Taskbar => "taskbar",
            Bell::Flash => "flash",
            Bell::None => "none",
        }
    }
}

/// `#[serde(default)]`: a setting missing from the file is taken from `Config::default()`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub font: String,
    pub font_size: f32,
    pub line_height: f32,
    /// `None`: the system's (see `shell()`).
    pub shell: Option<String>,
    pub ui_scale: f32,
    pub pixel_name: bool,
    pub cursor: CursorShape,
    pub cursor_blink: bool,
    pub copy_on_select: bool,
    /// macOS only: Option is Alt, not a key for letters (see `keys::mac_key_to_bytes`).
    pub option_as_alt: bool,
    pub scrollback: usize,
    pub bell: Bell,
    pub start_screen: bool,
    pub backdrop: backdrop::Choice,
    pub layout: Spacing,
    pub glass: Glass,
    pub colors: Theme,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font: crate::font::BUILT_IN.to_string(),
            font_size: 18.0,
            line_height: 1.0,
            shell: None,
            ui_scale: 1.0,
            pixel_name: false,
            cursor: CursorShape::Block,
            cursor_blink: true,
            copy_on_select: false,
            option_as_alt: false,
            scrollback: crate::term::SCROLLBACK_LIMIT,
            bell: Bell::Taskbar,
            start_screen: true,
            backdrop: backdrop::Choice::Auto,
            layout: Spacing::default(),
            glass: Glass::default(),
            colors: Theme::default(),
        }
    }
}

impl Config {
    /// The shell new tabs start: the config's, or the system's.
    pub fn shell(&self) -> String {
        self.shell.clone().unwrap_or_else(crate::shell::system)
    }
}

/// Kel's folder, with the config and the projects:
/// - Windows: `%APPDATA%\kel`, e.g. `C:\Users\you\AppData\Roaming\kel`;
/// - Linux and macOS: `~/.config/kel`, or `$XDG_CONFIG_HOME/kel` if it's set.
///   macOS has `~/Library/Application Support` for programs' settings, but
///   terminals (kitty, Alacritty, WezTerm, Ghostty) and the tools in them (nvim,
///   git) keep theirs in `~/.config`, where your dotfiles are.
pub fn dir() -> anyhow::Result<PathBuf> {
    kel_dir(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
}

/// `dir`, with `var` reading the environment.
fn kel_dir(var: impl Fn(&str) -> Option<OsString>) -> anyhow::Result<PathBuf> {
    if cfg!(windows) {
        let appdata = var("APPDATA").context("APPDATA is not set")?;
        return Ok(PathBuf::from(appdata).join("kel"));
    }
    // Only a full path: the standard says to ignore a relative one.
    let xdg = var("XDG_CONFIG_HOME").map(PathBuf::from);
    if let Some(xdg) = xdg.filter(|path| path.is_absolute()) {
        return Ok(xdg.join("kel"));
    }
    let home = var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config").join("kel"))
}

/// Where the config file lives, e.g. `C:\Users\you\AppData\Roaming\kel\config.toml`
/// or `/home/you/.config/kel/config.toml`.
pub fn path() -> anyhow::Result<PathBuf> {
    Ok(dir()?.join("config.toml"))
}

/// When the config file was last saved. `None` if there is no file.
/// Kel compares it to the time from the last load to notice a new save.
pub fn modified() -> Option<SystemTime> {
    std::fs::metadata(path().ok()?).ok()?.modified().ok()
}

/// Reads the config file, writing the default one first if there is none yet.
pub fn load() -> anyhow::Result<Config> {
    let path = path()?;
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, DEFAULT_CONFIG)?;
    }
    let text = std::fs::read_to_string(&path)?;
    parse_toml("config.toml", &text)
}

/// Reads `text`, the contents of the TOML file `file` (its name, for errors).
pub fn parse_toml<T: DeserializeOwned>(file: &str, text: &str) -> anyhow::Result<T> {
    // toml's own message has several lines with a drawing of the bad line.
    // Kel shows errors on one line, so: where, and why.
    // e.g. "config.toml, line 3: invalid type: string "big", expected f32"
    toml::from_str(text).map_err(|e| {
        let line = e.span().map_or(1, |span| line_at(text, span.start));
        anyhow::anyhow!("{file}, line {line}: {}", e.message())
    })
}

/// The line number (from 1) of byte `offset` in `text`: one more than the
/// line breaks before it.
fn line_at(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment `kel_dir` sees: these variables, no others.
    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            vars.iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[cfg(windows)]
    #[test]
    fn kel_dir_on_windows() {
        let appdata = env(&[("APPDATA", r"C:\Users\you\AppData\Roaming")]);
        let dir = kel_dir(appdata).unwrap();
        assert_eq!(dir, PathBuf::from(r"C:\Users\you\AppData\Roaming\kel"));
        assert!(kel_dir(env(&[])).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn kel_dir_on_linux_and_macos() {
        let home = env(&[("HOME", "/home/you")]);
        assert_eq!(
            kel_dir(home).unwrap(),
            PathBuf::from("/home/you/.config/kel")
        );
        let xdg = env(&[("HOME", "/home/you"), ("XDG_CONFIG_HOME", "/data/config")]);
        assert_eq!(kel_dir(xdg).unwrap(), PathBuf::from("/data/config/kel"));
        // A relative XDG_CONFIG_HOME doesn't count.
        let relative = env(&[("HOME", "/home/you"), ("XDG_CONFIG_HOME", "config")]);
        let dir = kel_dir(relative).unwrap();
        assert_eq!(dir, PathBuf::from("/home/you/.config/kel"));
        assert!(kel_dir(env(&[])).is_err());
    }

    #[test]
    fn default_file_matches_defaults() {
        assert_eq!(
            toml::from_str::<Config>(DEFAULT_CONFIG).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn missing_setting_keeps_its_default() {
        let config: Config = toml::from_str("font_size = 24").unwrap();
        assert_eq!(config.font_size, 24.0);
        assert_eq!(config.shell, None); // the system's
        assert_eq!(config.shell(), crate::shell::system());
        let config: Config = toml::from_str("shell = \"pwsh.exe -NoLogo\"").unwrap();
        assert_eq!(config.shell(), "pwsh.exe -NoLogo");
    }

    #[test]
    fn one_color_changes_the_rest_stay() {
        let config: Config = toml::from_str("[colors]\nbackground = \"#000000\"").unwrap();
        assert_eq!(config.colors.background, 0x000000);
        assert_eq!(config.colors.foreground, Theme::default().foreground);
    }

    #[test]
    fn project_colors() {
        let config: Config = toml::from_str(
            "[colors]
project_colors = [\"#112233\", \"#445566\"]",
        )
        .unwrap();
        assert_eq!(config.colors.project_colors, [0x112233, 0x445566]);
        assert_eq!(config.colors.done, Theme::default().done);
        let bad = "[colors]
project_colors = [\"#112233\", \"blue\"]";
        assert!(toml::from_str::<Config>(bad).is_err());
    }

    #[test]
    fn bad_colors_are_errors() {
        assert!(toml::from_str::<Config>("[colors]\nbackground = \"black\"").is_err());
        assert!(toml::from_str::<Config>("[colors]\npalette = [\"#000000\"]").is_err()); // not 16
    }

    #[test]
    fn line_numbers() {
        let text = "a = 1\nb = 2\nc = 3\n";
        assert_eq!(line_at(text, 0), 1); // "a"
        assert_eq!(line_at(text, 12), 3); // "c", the very start of line 3
    }

    #[test]
    fn glass() {
        use crate::glass::Effect;
        // Acrylic by default; "none": solid.
        assert_eq!(Config::default().glass.effect, Effect::Acrylic);
        let config: Config = toml::from_str("[glass]\neffect = \"none\"").unwrap();
        assert_eq!(config.glass.effect, Effect::None);
        assert_eq!(config.glass.opacity, Glass::default().opacity);
        assert!(toml::from_str::<Config>("[glass]\neffect = \"frosted\"").is_err());
        // The solid background under lazygit: on, the theme's color; or off, or black.
        assert!(Config::default().glass.solid_under_apps);
        let config: Config =
            toml::from_str("[glass]\nsolid_under_apps = false\nsolid_color = \"#000000\"").unwrap();
        assert!(!config.glass.solid_under_apps);
        assert_eq!(config.glass.solid_color, Some(0));
    }

    #[test]
    fn line_height() {
        assert_eq!(Config::default().line_height, 1.0);
        let config: Config = toml::from_str("line_height = 1.2").unwrap();
        assert_eq!(config.line_height, 1.2);
    }

    #[test]
    fn cursor_blink() {
        assert!(Config::default().cursor_blink);
        let config: Config = toml::from_str("cursor_blink = false").unwrap();
        assert!(!config.cursor_blink);
    }

    #[test]
    fn scrollback() {
        assert_eq!(Config::default().scrollback, 10_000);
        let config: Config = toml::from_str("scrollback = 50000").unwrap();
        assert_eq!(config.scrollback, 50_000);
        assert!(toml::from_str::<Config>("scrollback = -1").is_err());
    }

    #[test]
    fn copy_on_select() {
        assert!(!Config::default().copy_on_select);
        let config: Config = toml::from_str("copy_on_select = true").unwrap();
        assert!(config.copy_on_select);
    }

    #[test]
    fn bell() {
        assert_eq!(Config::default().bell, Bell::Taskbar);
        let config: Config = toml::from_str("bell = \"flash\"").unwrap();
        assert_eq!(config.bell, Bell::Flash);
        assert_eq!(config.bell.name(), "flash");
        assert!(toml::from_str::<Config>("bell = \"loud\"").is_err());
    }

    #[test]
    fn start_screen() {
        assert!(Config::default().start_screen);
        let config: Config = toml::from_str("start_screen = false").unwrap();
        assert!(!config.start_screen);
    }

    #[test]
    fn backdrop() {
        use crate::backdrop::Choice;
        assert_eq!(Config::default().backdrop, Choice::Auto);
        let config: Config = toml::from_str("backdrop = \"mountains\"").unwrap();
        assert_eq!(config.backdrop, Choice::Mountains);
        assert_eq!(config.backdrop.name(), "mountains");
        assert!(toml::from_str::<Config>("backdrop = \"forest\"").is_err());
    }

    #[test]
    fn wrong_type_is_an_error() {
        assert!(toml::from_str::<Config>("font_size = \"big\"").is_err());
    }
}
