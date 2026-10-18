//! Tiny `key = value` config at `$XDG_CONFIG_HOME/au.sam.Calc/config.toml`.
//! No TOML crate and no GSettings, so no schema has to be installed.

use std::fs;
use std::path::PathBuf;

use crate::APP_ID;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub width: i32,
    pub height: i32,
    pub advanced: bool,
    pub radians: bool,
    pub history_visible: bool,
    pub sidebar: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 370,
            height: 580,
            advanced: false,
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
                "mode" => cfg.advanced = value == "advanced",
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
            if self.advanced { "advanced" } else { "basic" },
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
            advanced: true,
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
