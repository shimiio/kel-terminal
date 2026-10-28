//! The config file: `%APPDATA%\kel\config.toml`.
//!
//! On the first start Kel writes the default file, with a comment for every setting.
//! A setting missing from the file keeps its default value.

use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

/// The file Kel writes on the first start. Its values must match `Config::default()`
/// (a test checks it).
const DEFAULT_CONFIG: &str = r#"# Kel config. Lines starting with # are comments.
# Delete a setting to go back to its default value.

# Font size in pixels. Ctrl+= / Ctrl+- change it until Kel closes, Ctrl+0 comes back to this.
font_size = 18

# The program Kel starts, e.g. "pwsh.exe" for PowerShell 7, or "cmd.exe".
shell = "powershell.exe"
"#;

/// `#[serde(default)]`: a setting missing from the file is taken from `Config::default()`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub font_size: f32,
    pub shell: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font_size: 18.0,
            shell: "powershell.exe".to_string(),
        }
    }
}

/// Where the config file lives, e.g. `C:\Users\you\AppData\Roaming\kel\config.toml`.
pub fn path() -> anyhow::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(appdata).join("kel").join("config.toml"))
}

/// Reads the config file, writing the default one first if there is none yet.
pub fn load() -> anyhow::Result<Config> {
    let path = path()?;
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, DEFAULT_CONFIG)?;
    }
    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).with_context(|| format!("{}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_matches_defaults() {
        assert_eq!(toml::from_str::<Config>(DEFAULT_CONFIG).unwrap(), Config::default());
    }

    #[test]
    fn missing_setting_keeps_its_default() {
        let config: Config = toml::from_str("font_size = 24").unwrap();
        assert_eq!(config.font_size, 24.0);
        assert_eq!(config.shell, "powershell.exe");
    }

    #[test]
    fn wrong_type_is_an_error() {
        assert!(toml::from_str::<Config>("font_size = \"big\"").is_err());
    }
}
