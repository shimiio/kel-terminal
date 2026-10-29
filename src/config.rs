//! The config file: `%APPDATA%\kel\config.toml`.
//!
//! On the first start Kel writes the default file, with a comment for every setting.
//! A setting missing from the file keeps its default value.

use crate::layout::Spacing;
use crate::theme::Theme;
use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::SystemTime;

/// The file Kel writes on the first start. Its values must match `Config::default()`
/// (a test checks it).
// `r##"..."##`: a raw string, ending at `"##`. The colors hold `"#`, which would
// end a plain `r#"..."#` string early.
const DEFAULT_CONFIG: &str = r##"# Kel config. Lines starting with # are comments.
# Ctrl+, opens this file. Saved changes apply right away, unless a setting says otherwise.
# Delete a setting to go back to its default value.

# Any installed font, by the name Windows shows in Settings > Fonts.
# A Nerd Font (e.g. "JetBrainsMono Nerd Font") also has icons, which nvim plugins use.
font = "Consolas"

# Font size in pixels. Ctrl+= / Ctrl+- change it until Kel closes, Ctrl+0 comes back to this.
font_size = 18

# The program Kel starts, e.g. "pwsh.exe" for PowerShell 7, or "cmd.exe".
# Applies after a restart.
shell = "powershell.exe"

# Spaces around the terminal, in pixels. The terminal is a panel inside the window:
# margin (window edge → panel), border (the panel's line), padding (line → text).
# All 0: the text fills the window, like a plain terminal.
[layout]
margin = 8
border = 1
padding = 6
tab_height = 32   # the tab bar above the panel
tab_width = 180   # narrower when there are many tabs

# Colors, as "#rrggbb". Programs ask for "red" or "default text"; these say how they look.
# Programs that pick exact colors themselves (like nvim color schemes) don't use them.
[colors]
foreground = "#d4d4d4"
background = "#1e1e1e"
cursor = "#808080"
selection = "#264f78"
frame = "#141414"    # the window around the panel
border = "#333333"   # the panel's border line
accent = "#569cd6"   # the tab icons
# 16 colors: black, red, green, yellow, blue, magenta, cyan, white,
# then the bright versions of the same 8.
palette = [
    "#0c0c0c", "#c50f1f", "#13a10e", "#c19c00", "#0037da", "#881798", "#3a96dd", "#cccccc",
    "#767676", "#e74856", "#16c60c", "#f9f1a5", "#3b78ff", "#b4009e", "#61d6d6", "#f2f2f2",
]
"##;

/// `#[serde(default)]`: a setting missing from the file is taken from `Config::default()`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub font: String,
    pub font_size: f32,
    pub shell: String,
    pub layout: Spacing,
    pub colors: Theme,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font: "Consolas".to_string(),
            font_size: 18.0,
            shell: "powershell.exe".to_string(),
            layout: Spacing::default(),
            colors: Theme::default(),
        }
    }
}

/// Where the config file lives, e.g. `C:\Users\you\AppData\Roaming\kel\config.toml`.
pub fn path() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(appdata).join("kel").join("config.toml"))
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
    // toml's own message has several lines with a drawing of the bad line.
    // Kel shows errors on one line, so: where, and why.
    // e.g. "config.toml, line 3: invalid type: string "big", expected f32"
    toml::from_str(&text).map_err(|e| {
        let line = e.span().map_or(1, |span| line_at(&text, span.start));
        anyhow::anyhow!("config.toml, line {line}: {}", e.message())
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
    fn wrong_type_is_an_error() {
        assert!(toml::from_str::<Config>("font_size = \"big\"").is_err());
    }
}
