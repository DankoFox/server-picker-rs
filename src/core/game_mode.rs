use serde::{Deserialize, Serialize};
use std::fmt;

const DEFAULT_RESPONSE_URL_TEMPLATE: &str =
    "https://api.steampowered.com/ISteamApps/GetSDRConfig/v1/?appid={app_id}";

/// How a game mode filters server descriptions by keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeywordFilterMode {
    /// Only keep servers whose description contains at least one keyword.
    Include,
    /// Drop servers whose description contains any keyword.
    Exclude,
    /// No keyword filtering.
    #[default]
    None,
}

impl fmt::Display for KeywordFilterMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeywordFilterMode::Include => write!(f, "include"),
            KeywordFilterMode::Exclude => write!(f, "exclude"),
            KeywordFilterMode::None => write!(f, "none"),
        }
    }
}

/// Static configuration for a supported game mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameModeDefinition {
    // Note: manual Default is provided below so the response URL template is
    // populated correctly.
    #[serde(rename = "gameMode")]
    pub game_mode: String,
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "appId")]
    pub app_id: u32,
    #[serde(rename = "keywordFilterMode", default)]
    pub keyword_filter_mode: KeywordFilterMode,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(
        rename = "responseUrlTemplate",
        default = "default_response_url_template"
    )]
    pub response_url_template: String,
}

fn default_response_url_template() -> String {
    DEFAULT_RESPONSE_URL_TEMPLATE.to_string()
}

impl Default for GameModeDefinition {
    fn default() -> Self {
        Self {
            game_mode: String::new(),
            id: String::new(),
            display_name: String::new(),
            app_id: 0,
            keyword_filter_mode: KeywordFilterMode::default(),
            keywords: Vec::new(),
            response_url_template: default_response_url_template(),
        }
    }
}

impl GameModeDefinition {
    /// Build the Valve SDR config URL for this game mode.
    pub fn sdr_config_url(&self) -> String {
        self.response_url_template
            .replace("{app_id}", &self.app_id.to_string())
            .replace("{0}", &self.app_id.to_string())
    }
}

/// Errors that can occur while loading game mode definitions.
#[derive(Debug, thiserror::Error)]
pub enum DefinitionError {
    #[error("Failed to parse ServerDefinitions.json: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Load the embedded default definitions.
pub fn load_default_definitions() -> Result<Vec<GameModeDefinition>, DefinitionError> {
    let json = include_str!("../../assets/ServerDefinitions.json");
    serde_json::from_str(json).map_err(DefinitionError::from)
}

/// Load definitions from a JSON string.
pub fn load_definitions_from_str(json: &str) -> Result<Vec<GameModeDefinition>, DefinitionError> {
    serde_json::from_str(json).map_err(DefinitionError::from)
}

/// Load definitions from a file, falling back to the embedded defaults if the
/// file does not exist.
pub fn load_definitions_from_path_or_default(
    path: &std::path::Path,
) -> Result<Vec<GameModeDefinition>, DefinitionError> {
    if path.exists() {
        let json = std::fs::read_to_string(path)?;
        load_definitions_from_str(&json)
    } else {
        load_default_definitions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_definitions_load_successfully() {
        let defs = load_default_definitions().expect("default definitions should parse");
        assert!(!defs.is_empty());

        let cs2 = defs
            .iter()
            .find(|d| d.id == "cs2")
            .expect("cs2 should exist");
        assert_eq!(cs2.app_id, 730);
        assert_eq!(cs2.keyword_filter_mode, KeywordFilterMode::Exclude);
        assert_eq!(cs2.keywords, vec!["China"]);
    }

    #[test]
    fn sdr_config_url_replaces_both_template_styles() {
        let def = GameModeDefinition {
            game_mode: "Test".to_string(),
            id: "test".to_string(),
            display_name: "Test".to_string(),
            app_id: 123,
            keyword_filter_mode: KeywordFilterMode::None,
            keywords: vec![],
            response_url_template: "https://example.com/?appid={app_id}".to_string(),
        };
        assert_eq!(
            def.sdr_config_url(),
            "https://example.com/?appid=123".to_string()
        );

        let def2 = GameModeDefinition {
            response_url_template: "https://example.com/?appid={0}".to_string(),
            ..def
        };
        assert_eq!(
            def2.sdr_config_url(),
            "https://example.com/?appid=123".to_string()
        );
    }

    #[test]
    fn keyword_filter_mode_round_trips_through_json() {
        let modes = vec![
            KeywordFilterMode::Include,
            KeywordFilterMode::Exclude,
            KeywordFilterMode::None,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: KeywordFilterMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, parsed);
        }
    }
}
