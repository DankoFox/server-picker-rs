use crate::core::filter::build_server_data;
use crate::core::filter::{RawPop, RawRelay};
use crate::core::game_mode::GameModeDefinition;
use crate::core::server::ServerData;
use serde::Deserialize;
use std::collections::HashMap;

/// Errors that can occur when fetching or parsing SDR config from Valve.
#[derive(Debug, thiserror::Error)]
pub enum ValveApiError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Invalid JSON response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Server relay data unavailable: missing '{0}' field")]
    MissingField(&'static str),
    #[error("Failed to load servers. Verify your internet connection and firewall, and run the app as admin/sudo if required.")]
    LoadFailed,
}

/// Raw SDR config as returned by the Valve API.
#[derive(Debug, Clone, Deserialize)]
struct RawSdrConfig {
    #[serde(default)]
    revision: serde_json::Value,
    #[serde(default)]
    pops: HashMap<String, RawPopDefinition>,
}

fn revision_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RawPopDefinition {
    desc: String,
    #[serde(default)]
    relays: Vec<RawRelayDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRelayDefinition {
    #[serde(default)]
    ipv4: String,
    #[serde(default = "default_port_range")]
    port_range: Vec<u16>,
}

fn default_port_range() -> Vec<u16> {
    vec![27015, 27015]
}

impl From<RawSdrConfig> for Vec<RawPop> {
    fn from(config: RawSdrConfig) -> Self {
        config
            .pops
            .into_iter()
            .map(|(name, pop)| RawPop {
                name,
                desc: pop.desc,
                relays: pop
                    .relays
                    .into_iter()
                    .map(|relay| {
                        let min = relay.port_range.first().copied().unwrap_or(27015);
                        let max = relay.port_range.get(1).copied().unwrap_or(min);
                        RawRelay {
                            ipv4: relay.ipv4,
                            port_range_min: min,
                            port_range_max: max,
                        }
                    })
                    .collect(),
            })
            .collect()
    }
}

/// Trait abstracting the HTTP client so tests can avoid real network calls.
#[async_trait::async_trait]
pub trait ValveApiClient: Send + Sync {
    async fn fetch_sdr_config(&self, app_id: u32) -> Result<String, ValveApiError>;
}

/// Live HTTP client that talks to the Valve API.
pub struct ReqwestValveApiClient {
    client: reqwest::Client,
}

impl ReqwestValveApiClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl ValveApiClient for ReqwestValveApiClient {
    async fn fetch_sdr_config(&self, app_id: u32) -> Result<String, ValveApiError> {
        let url = format!(
            "https://api.steampowered.com/ISteamApps/GetSDRConfig/v1/?appid={}",
            app_id
        );
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(ValveApiError::LoadFailed);
        }

        Ok(response.text().await?)
    }
}

/// Fetch and parse the SDR config for the given game mode definition.
pub async fn load_server_data(
    client: &dyn ValveApiClient,
    definition: &GameModeDefinition,
) -> Result<ServerData, ValveApiError> {
    let body = client.fetch_sdr_config(definition.app_id).await?;
    parse_server_data(&body, definition)
}

/// Parse a JSON body into `ServerData` using the provided definition.
pub fn parse_server_data(
    body: &str,
    definition: &GameModeDefinition,
) -> Result<ServerData, ValveApiError> {
    let raw: RawSdrConfig = serde_json::from_str(body)?;

    let revision = revision_to_string(&raw.revision);
    if revision.is_empty() {
        return Err(ValveApiError::MissingField("revision"));
    }

    let pops: Vec<RawPop> = raw.into();

    if pops.is_empty() {
        return Err(ValveApiError::MissingField("pops"));
    }

    Ok(build_server_data(revision, pops, definition))
}

// Simpler in-app mock client for tests.
#[cfg(test)]
pub struct MockValveApiClient {
    response: String,
}

#[cfg(test)]
impl MockValveApiClient {
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl ValveApiClient for MockValveApiClient {
    async fn fetch_sdr_config(&self, _app_id: u32) -> Result<String, ValveApiError> {
        Ok(self.response.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game_mode::{GameModeDefinition, KeywordFilterMode};

    fn sample_json() -> String {
        r#"{
            "revision": "12345",
            "pops": {
                "tyo": {
                    "desc": "Japan Tokyo",
                    "relays": [
                        { "ipv4": "192.0.2.1", "port_range": [27015, 27060] },
                        { "ipv4": "192.0.2.2", "port_range": [27015, 27060] }
                    ]
                },
                "hkg": {
                    "desc": "Hong Kong",
                    "relays": [
                        { "ipv4": "198.51.100.1", "port_range": [27015, 27140] }
                    ]
                },
                "sha": {
                    "desc": "China Shanghai",
                    "relays": [
                        { "ipv4": "203.0.113.1", "port_range": [27015, 27060] }
                    ]
                }
            }
        }"#
        .to_string()
    }

    fn cs2_definition() -> GameModeDefinition {
        GameModeDefinition {
            game_mode: "Counter Strike 2".to_string(),
            id: "cs2".to_string(),
            display_name: "CS2".to_string(),
            app_id: 730,
            keyword_filter_mode: KeywordFilterMode::Exclude,
            keywords: vec!["China".to_string()],
            response_url_template: GameModeDefinition::default().response_url_template,
        }
    }

    #[test]
    fn parse_server_data_extracts_revision() {
        let def = cs2_definition();
        let data = parse_server_data(&sample_json(), &def).unwrap();
        assert_eq!(data.revision, "12345");
    }

    #[test]
    fn parse_server_data_excludes_china_for_cs2() {
        let def = cs2_definition();
        let data = parse_server_data(&sample_json(), &def).unwrap();

        let descriptions: Vec<&str> = data
            .servers
            .iter()
            .map(|s| s.description.as_str())
            .collect();
        assert!(descriptions.contains(&"Japan Tokyo"));
        assert!(descriptions.contains(&"Hong Kong"));
        assert!(!descriptions.contains(&"China Shanghai"));
    }

    #[test]
    fn parse_server_data_sorts_by_region() {
        let def = cs2_definition();
        let data = parse_server_data(&sample_json(), &def).unwrap();

        let hong_kong = data
            .servers
            .iter()
            .find(|s| s.description == "Hong Kong")
            .expect("Hong Kong server should exist");
        assert_eq!(hong_kong.name, "hkg");
        assert_eq!(hong_kong.relays.len(), 1);
        assert_eq!(hong_kong.region_name(), "Hong Kong");
    }

    #[test]
    fn parse_server_data_rejects_missing_revision() {
        let json = r#"{"pops": {}}"#;
        let def = cs2_definition();
        let err = parse_server_data(json, &def).unwrap_err();
        assert!(matches!(err, ValveApiError::MissingField("revision")));
    }

    #[test]
    fn parse_server_data_rejects_missing_pops() {
        let json = r#"{"revision": "1"}"#;
        let def = cs2_definition();
        let err = parse_server_data(json, &def).unwrap_err();
        assert!(matches!(err, ValveApiError::MissingField("pops")));
    }

    #[tokio::test]
    async fn mock_client_returns_config() {
        let client = MockValveApiClient::new(sample_json());
        let def = cs2_definition();
        let data = load_server_data(&client, &def).await.unwrap();
        assert_eq!(data.revision, "12345");
    }

    #[test]
    fn parse_server_data_preserves_singapore_relays() {
        let json = r#"{
            "revision": 1794268433,
            "pops": {
                "sgp": {
                    "desc": "Singapore",
                    "relays": [
                        { "ipv4": "103.10.124.116", "port_range": [27015, 27060] },
                        { "ipv4": "103.10.124.117", "port_range": [27015, 27060] },
                        { "ipv4": "103.10.124.118", "port_range": [27015, 27060] }
                    ]
                }
            }
        }"#;
        let def = cs2_definition();
        let data = parse_server_data(json, &def).unwrap();

        let sgp = data
            .servers
            .iter()
            .find(|s| s.name == "sgp")
            .expect("Singapore server should exist");
        assert_eq!(sgp.relays.len(), 3);
        assert_eq!(sgp.relays[0].ipv4, "103.10.124.116");
        assert_eq!(sgp.relays[0].port_range_min, 27015);
        assert_eq!(sgp.relays[0].port_range_max, 27060);
    }
}
