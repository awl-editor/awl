use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    /// Path to a theme TOML file. Tilde and env vars are not expanded by the
    /// loader — use an absolute path or a path relative to the config dir.
    pub theme: Option<PathBuf>,
}

impl Config {
    /// Load `~/.config/awl/config.toml` (or `$XDG_CONFIG_HOME/awl/config.toml`).
    /// Missing file → silent default. Parse error → log to stderr, use default.
    pub fn load() -> Self {
        let path = Self::path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };
        match toml::from_str(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("awl: config parse error ({path:?}): {e}");
                Self::default()
            }
        }
    }

    pub fn path() -> PathBuf {
        Self::dir().join("config.toml")
    }

    pub fn dir() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join(".config"))
            .join("awl")
    }
}
