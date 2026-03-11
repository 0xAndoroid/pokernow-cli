use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub unify: HashMap<String, Vec<String>>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn expanded_files(&self) -> Vec<String> {
        self.files.iter().map(|f| expand_tilde(f)).collect()
    }

    pub fn unify_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (canonical, aliases) in &self.unify {
            for alias in aliases {
                if alias != canonical {
                    map.insert(alias.clone(), canonical.clone());
                }
            }
        }
        map
    }
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest).to_string_lossy().into_owned();
    }
    path.to_string()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
