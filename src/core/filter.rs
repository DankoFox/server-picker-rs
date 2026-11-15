use crate::core::game_mode::{GameModeDefinition, KeywordFilterMode};
use crate::core::server::{Relay, Server, ServerData};

/// Determine whether a server description is accepted by the game mode's
/// keyword filter.
pub fn is_server_accepted(description: &str, definition: &GameModeDefinition) -> bool {
    let desc_lower = description.to_lowercase();
    match definition.keyword_filter_mode {
        KeywordFilterMode::Include => definition
            .keywords
            .iter()
            .any(|keyword| desc_lower.contains(&keyword.to_lowercase())),
        KeywordFilterMode::Exclude => definition
            .keywords
            .iter()
            .all(|keyword| !desc_lower.contains(&keyword.to_lowercase())),
        KeywordFilterMode::None => true,
    }
}

/// Build the filtered server list from the raw Valve POP map.
pub fn build_server_data(
    revision: impl Into<String>,
    pops: Vec<RawPop>,
    definition: &GameModeDefinition,
) -> ServerData {
    let mut servers: Vec<Server> = pops
        .into_iter()
        .filter(|pop| is_server_accepted(&pop.desc, definition))
        .map(|pop| {
            let mut server = Server::new(&pop.name, &pop.desc);
            server.flag = Server::country_code(&pop.desc).to_string();
            server.relays = pop
                .relays
                .into_iter()
                .map(|r| Relay::with_port_range(r.ipv4, r.port_range_min, r.port_range_max))
                .collect();
            server
        })
        .collect();

    // Stable, deterministic ordering for tests and UI.
    servers.sort_by(|a, b| {
        a.region_group()
            .cmp(b.region_group())
            .then_with(|| a.region_name().cmp(b.region_name()))
            .then_with(|| a.name.cmp(&b.name))
    });

    ServerData {
        revision: revision.into(),
        servers,
    }
}

/// Intermediate representation of a Valve POP before filtering/clustering.
#[derive(Debug, Clone)]
pub struct RawPop {
    pub name: String,
    pub desc: String,
    pub relays: Vec<RawRelay>,
}

#[derive(Debug, Clone)]
pub struct RawRelay {
    pub ipv4: String,
    pub port_range_min: u16,
    pub port_range_max: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def_with_filter(mode: KeywordFilterMode, keywords: &[&str]) -> GameModeDefinition {
        GameModeDefinition {
            game_mode: "Test".to_string(),
            id: "test".to_string(),
            display_name: "Test".to_string(),
            app_id: 0,
            keyword_filter_mode: mode,
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            response_url_template: GameModeDefinition::default().response_url_template,
        }
    }

    #[test]
    fn exclude_filter_drops_matching_servers() {
        let def = def_with_filter(KeywordFilterMode::Exclude, &["China"]);
        assert!(is_server_accepted("Hong Kong", &def));
        assert!(!is_server_accepted("China East", &def));
    }

    #[test]
    fn include_filter_keeps_only_matching_servers() {
        let def = def_with_filter(KeywordFilterMode::Include, &["China"]);
        assert!(!is_server_accepted("Hong Kong", &def));
        assert!(is_server_accepted("China East", &def));
    }

    #[test]
    fn none_filter_accepts_everything() {
        let def = def_with_filter(KeywordFilterMode::None, &[]);
        assert!(is_server_accepted("Anything", &def));
    }

    #[test]
    fn build_server_data_filters_and_sorts_by_region() {
        let def = def_with_filter(KeywordFilterMode::None, &[]);

        let pops = vec![
            RawPop {
                name: "sto".to_string(),
                desc: "Stockholm (Sweden)".to_string(),
                relays: vec![RawRelay {
                    ipv4: "1.1.1.1".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27060,
                }],
            },
            RawPop {
                name: "hkg".to_string(),
                desc: "Hong Kong".to_string(),
                relays: vec![RawRelay {
                    ipv4: "2.2.2.2".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27140,
                }],
            },
            RawPop {
                name: "ams".to_string(),
                desc: "Amsterdam (Netherlands)".to_string(),
                relays: vec![RawRelay {
                    ipv4: "3.3.3.3".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27060,
                }],
            },
        ];

        let data = build_server_data("1", pops, &def);

        assert_eq!(data.servers.len(), 3);
        assert_eq!(data.servers[0].region_group(), "Asia");
        assert_eq!(data.servers[0].region_name(), "Hong Kong");
        assert_eq!(data.servers[1].region_group(), "Europe");
        assert_eq!(data.servers[1].region_name(), "Netherlands");
        assert_eq!(data.servers[2].region_group(), "Europe");
        assert_eq!(data.servers[2].region_name(), "Sweden");
    }

    #[test]
    fn filter_is_case_insensitive() {
        let def = def_with_filter(KeywordFilterMode::Exclude, &["china"]);
        assert!(!is_server_accepted("CHINA East", &def));
        assert!(!is_server_accepted("China East", &def));
    }
}
