//! Tiny `key = value` config at `$XDG_CONFIG_HOME/au.sam.Calc/config.toml`.
//! No TOML crate and no GSettings, so no schema has to be installed.

use std::fs;
use std::path::PathBuf;

use crate::APP_ID;

/// Which keys are shown: Basic, Mid (adds the `( ) x² √` row) or Advanced
/// (Mid plus the scientific block).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Basic,
    Mid,
    Advanced,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Basic => "basic",
            Mode::Mid => "mid",
            Mode::Advanced => "advanced",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Basic => "Basic",
            Mode::Mid => "Mid",
            Mode::Advanced => "Advanced",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "basic" => Some(Mode::Basic),
            "mid" => Some(Mode::Mid),
            "advanced" => Some(Mode::Advanced),
            _ => None,
        }
    }

    /// Keys with a level above this are hidden.
    pub fn level(self) -> u8 {
        match self {
            Mode::Basic => 0,
            Mode::Mid => 1,
            Mode::Advanced => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub width: i32,
    pub height: i32,
    pub mode: Mode,
    pub radians: bool,
    pub history_visible: bool,
    pub sidebar: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 340,
            height: 660,
            mode: Mode::Basic,
            radians: false,
            history_visible: false,
            sidebar: 300,
        }
    }
}

fn path() -> PathBuf {
    gtk::glib::user_config_dir().join(APP_ID).join("config.toml")
}

impl Config {
    pub fn load() -> Self {
        fs::read_to_string(path())
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "width" => {
                    if let Ok(v) = value.parse::<i32>() {
                        cfg.width = v.clamp(300, 4000);
                    }
                }
                "height" => {
                    if let Ok(v) = value.parse::<i32>() {
                        cfg.height = v.clamp(360, 4000);
                    }
                }
                "mode" => cfg.mode = Mode::from_name(value).unwrap_or_default(),
                "angle" => cfg.radians = value == "rad",
                "sidebar" => {
                    if let Ok(v) = value.parse::<i32>() {
                        cfg.sidebar = v.clamp(220, 1200);
                    }
                }
                "history" => cfg.history_visible = value == "true",
                _ => {}
            }
        }
        cfg
    }

    pub fn render(&self) -> String {
        format!(
            "width = {}\nheight = {}\nmode = \"{}\"\nangle = \"{}\"\nhistory = {}\nsidebar = {}\n",
            self.width,
            self.height,
            self.mode.name(),
            if self.radians { "rad" } else { "deg" },
            self.history_visible,
            self.sidebar,
        )
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(p, self.render());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let c = Config {
            width: 800,
            height: 600,
            mode: Mode::Mid,
            radians: true,
            history_visible: true,
            sidebar: 333,
        };
        assert_eq!(Config::parse(&c.render()), c);
    }

    #[test]
    fn garbage_is_tolerated() {
        assert_eq!(Config::parse("%%%\nwidth = abc\n= =\n"), Config::default());
        assert_eq!(Config::parse("width = 99999").width, 4000);
    }
}
