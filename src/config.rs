use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct BlindRemap {
    pub from: [f64; 2],
    pub to: [f64; 2],
}

#[derive(Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub unify: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub blind_remap: Vec<BlindRemap>,
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

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config() {
        let toml = r#"
files = ["~/hands/game1.json", "~/hands/game2.json"]

[unify]
alice = ["alice", "alice2"]
bob = ["bob", "robert"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert_eq!(config.unify.len(), 2);
        assert_eq!(config.unify["alice"], vec!["alice", "alice2"]);
    }

    #[test]
    fn parse_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.files.is_empty());
        assert!(config.unify.is_empty());
    }

    #[test]
    fn parse_files_only() {
        let toml = r#"files = ["a.json"]"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.files.len(), 1);
        assert!(config.unify.is_empty());
    }

    #[test]
    fn parse_unify_only() {
        let toml = r#"
[unify]
player = ["p1", "p2"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.files.is_empty());
        assert_eq!(config.unify.len(), 1);
    }

    #[test]
    fn tilde_expansion() {
        let config: Config =
            toml::from_str(r#"files = ["~/hands/test.json", "/abs/path.json"]"#).unwrap();
        let expanded = config.expanded_files();
        assert!(!expanded[0].starts_with('~'));
        assert!(expanded[0].ends_with("hands/test.json"));
        assert_eq!(expanded[1], "/abs/path.json");
    }

    #[test]
    fn no_tilde_expansion_for_non_tilde() {
        let config: Config = toml::from_str(r#"files = ["relative/path.json"]"#).unwrap();
        let expanded = config.expanded_files();
        assert_eq!(expanded[0], "relative/path.json");
    }

    #[test]
    fn unify_map_generation() {
        let toml = r#"
[unify]
alice = ["alice", "alice2", "alice3"]
bob = ["bob", "robert"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let map = config.unify_map();
        assert_eq!(map.get("alice2").unwrap(), "alice");
        assert_eq!(map.get("alice3").unwrap(), "alice");
        assert_eq!(map.get("robert").unwrap(), "bob");
        // canonical names pointing to themselves are excluded
        assert!(!map.contains_key("alice"));
        assert!(!map.contains_key("bob"));
    }

    #[test]
    fn unify_map_empty() {
        let config = Config::default();
        let map = config.unify_map();
        assert!(map.is_empty());
    }

    #[test]
    fn load_nonexistent_file() {
        let result = Config::load(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn load_valid_file() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, r#"files = ["test.json"]"#).unwrap();
        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.files, vec!["test.json"]);
    }

    #[test]
    fn load_invalid_toml() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "this is not valid toml [[[").unwrap();
        let result = Config::load(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn parse_blind_remap() {
        let toml = r"
[[blind_remap]]
from = [1.0, 1.0]
to = [1.0, 2.0]

[[blind_remap]]
from = [0.5, 0.5]
to = [0.5, 1.0]
";
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.blind_remap.len(), 2);
        assert_eq!(config.blind_remap[0].from, [1.0, 1.0]);
        assert_eq!(config.blind_remap[0].to, [1.0, 2.0]);
        assert_eq!(config.blind_remap[1].from, [0.5, 0.5]);
        assert_eq!(config.blind_remap[1].to, [0.5, 1.0]);
    }

    #[test]
    fn parse_config_without_blind_remap() {
        let toml = r#"files = ["test.json"]"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.blind_remap.is_empty());
    }

    #[test]
    fn parse_full_config_with_blind_remap() {
        let toml = r#"
files = ["game.json"]

[unify]
alice = ["alice", "alice2"]

[[blind_remap]]
from = [1.0, 1.0]
to = [1.0, 2.0]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.files.len(), 1);
        assert_eq!(config.unify.len(), 1);
        assert_eq!(config.blind_remap.len(), 1);
    }
}
