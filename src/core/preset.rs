use serde::{Deserialize, Serialize};

/// A user-defined preset that stores blocked server keys for a game mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(rename = "gameMode")]
    pub game_mode: String,
    #[serde(rename = "isClustered")]
    pub is_clustered: bool,
    #[serde(
        rename = "searchText",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub search_text: String,
    #[serde(rename = "blockedServerKeys")]
    pub blocked_server_keys: Vec<String>,
}

impl Preset {
    /// Create a new empty preset.
    pub fn new(name: impl Into<String>, game_mode: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            game_mode: game_mode.into(),
            ..Self::default()
        }
    }

    /// Normalize blocked server keys: case-insensitive distinct sorted keys.
    pub fn normalize_blocked_keys(&mut self) {
        let mut keys: Vec<String> = self
            .blocked_server_keys
            .iter()
            .map(|k| k.to_lowercase())
            .collect();
        keys.sort();
        keys.dedup();
        self.blocked_server_keys = keys;
    }

    /// Case-insensitive equality check used for duplicate detection.
    pub fn matches(&self, game_mode: &str, name: &str) -> bool {
        self.game_mode.eq_ignore_ascii_case(game_mode) && self.name.eq_ignore_ascii_case(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_matches_is_case_insensitive() {
        let preset = Preset::new("My Preset", "Counter Strike 2");
        assert!(preset.matches("counter strike 2", "my preset"));
        assert!(!preset.matches("Counter Strike 2", "Other Preset"));
    }

    #[test]
    fn normalize_blocked_keys_sorts_and_dedupes() {
        let mut preset = Preset {
            blocked_server_keys: vec![
                "sto".to_string(),
                "HKG".to_string(),
                "sto".to_string(),
                "alpha".to_string(),
            ],
            ..Preset::default()
        };
        preset.normalize_blocked_keys();
        assert_eq!(preset.blocked_server_keys, vec!["alpha", "hkg", "sto"]);
    }
}
