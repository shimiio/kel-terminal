//! The config file: `%APPDATA%\kel\config.toml`.
//!
//! On the first start Kel writes the default file, with a comment for every setting.
//! A setting missing from the file keeps its default value.

use crate::glass::Glass;
use crate::layout::Spacing;
use crate::term::CursorShape;
use crate::theme::Theme;
use anyhow::Context;
use serde::Deserialize;
use serde::de::DeserializeOwned;
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

# The program Kel starts, e.g. "pwsh.exe" for PowerShell 7, or "cmd.exe".
# Applies after a restart.
shell = "powershell.exe"

# The size of Kel's own UI (tabs, buttons, the spaces below), on top of Windows'
# display scale (Settings > Display > Scale). e.g. 1.25: a quarter bigger.
# The terminal's text has font_size for that.
ui_scale = 1

# The project's name before the tabs in pixels, like Kel's logo. false: in the font.
pixel_name = true

# The cursor: "block" (the whole cell), "underline" or "bar".
# Programs can pick their own (nvim does); this is for the rest.
cursor = "block"

# Spaces around the terminal, in pixels. The terminal is a panel inside the window:
# margin (window edge → panel), border (the panel's line), padding (line → text).
# All 0: the text fills the window, like a plain terminal.
[layout]
margin = 8
border = 1
padding = 6
tab_height = 32   # the tab bar above the panel
tab_width = 180   # narrower when there are many tabs

# Glass: Kel see-through, Windows blurring what's behind it.
# effect: "acrylic" (what's behind, blurred), "mica" (a faint tint of the
# wallpaper, Windows 11), "none" (solid). Applies after a restart.
# Windows turns the blur off by itself at times (e.g. on battery): then the
# glass is a plain color.
[glass]
effect = "acrylic"
opacity = 0.85   # how much of the theme's colors cover the glass: 1 solid, 0 all glass
grain = 0.1      # a fine grain on the glass, like frosted glass: 0 none, 1 strong
solid_under_apps = true   # a solid background under lazygit, htop, Claude Code: they read better
# solid_color = "#000000" # its color; without it, the theme's background

# Colors, as "#rrggbb". Programs ask for "red" or "default text"; these say how they look.
# Programs that pick exact colors themselves (like nvim color schemes) don't use them.
[colors]
foreground = "#d4d4d4"
background = "#1e1e1e"
cursor = "#808080"
selection = "#474747"
frame = "#141414"    # the window around the panel
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
    "#3a3a3a", "#e06c75", "#98c379", "#e5c07b", "#7aa2e6", "#b39ae6", "#6cc4b8", "#c8c8c8",
    "#6e6e6e", "#f08a92", "#b4d99a", "#f2d49b", "#9dbcf2", "#cbb6f2", "#8fd9cf", "#ececec",
]
"##;

/// `#[serde(default)]`: a setting missing from the file is taken from `Config::default()`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub font: String,
    pub font_size: f32,
    pub shell: String,
    pub ui_scale: f32,
    pub pixel_name: bool,
    pub cursor: CursorShape,
    pub layout: Spacing,
    pub glass: Glass,
    pub colors: Theme,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font: crate::font::BUILT_IN.to_string(),
            font_size: 18.0,
            shell: "powershell.exe".to_string(),
            ui_scale: 1.0,
            pixel_name: true,
            cursor: CursorShape::Block,
            layout: Spacing::default(),
            glass: Glass::default(),
            colors: Theme::default(),
        }
    }
}

/// Kel's folder, e.g. `C:\Users\you\AppData\Roaming\kel`.
pub fn dir() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(appdata).join("kel"))
}

/// Where the config file lives, e.g. `C:\Users\you\AppData\Roaming\kel\config.toml`.
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
        assert_eq!(config.shell, "powershell.exe");
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
    fn wrong_type_is_an_error() {
        assert!(toml::from_str::<Config>("font_size = \"big\"").is_err());
    }
}
