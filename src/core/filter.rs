use crate::core::game_mode::{GameModeDefinition, KeywordFilterMode};
use crate::core::server::{Relay, Server, ServerData};
use std::collections::HashMap;

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

/// Build both unclustered and clustered server lists from the raw Valve POP
/// map.
pub fn build_server_data(
    revision: impl Into<String>,
    pops: Vec<RawPop>,
    definition: &GameModeDefinition,
) -> ServerData {
    let mut unclustered = Vec::new();
    let mut clusters: HashMap<String, Server> = HashMap::new();

    for pop in pops {
        if !is_server_accepted(&pop.desc, definition) {
            continue;
        }

        let mut server = Server::new(&pop.name, &pop.desc);
        server.flag = Server::country_code(&pop.desc).to_string();
        server.relays = pop
            .relays
            .into_iter()
            .map(|r| Relay::with_port_range(r.ipv4, r.port_range_min, r.port_range_max))
            .collect();

        unclustered.push(server.clone());

        if let Some(cluster_name) = find_cluster_keyword(&pop.desc, definition) {
            let cluster = clusters
                .entry(cluster_name.clone())
                .or_insert_with(|| Server {
                    flag: server.flag.clone(),
                    name: "cluster".to_string(),
                    description: cluster_name,
                    ..Server::default()
                });
            cluster.relays.extend(server.relays);
        } else {
            // When not clustered, the server still appears in the clustered
            // view as itself.
            clusters.insert(format!("{}-{}", pop.name, pop.desc), server);
        }
    }

    let mut clustered: Vec<Server> = clusters.into_values().collect();
    // Stable, deterministic ordering for tests and UI.
    clustered.sort_by(|a, b| {
        a.description
            .cmp(&b.description)
            .then_with(|| a.name.cmp(&b.name))
    });

    ServerData {
        revision: revision.into(),
        unclustered_servers: unclustered,
        clustered_servers: clustered,
    }
}

/// Find the first cluster keyword contained in the description.
fn find_cluster_keyword(description: &str, definition: &GameModeDefinition) -> Option<String> {
    let desc_lower = description.to_lowercase();
    definition
        .cluster_keywords
        .iter()
        .find(|keyword| desc_lower.contains(&keyword.to_lowercase()))
        .cloned()
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
            cluster_keywords: vec!["Sweden".to_string(), "Hong Kong".to_string()],
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
    fn clustering_merges_matching_servers() {
        let def = GameModeDefinition {
            cluster_keywords: vec!["Sweden".to_string(), "Hong Kong".to_string()],
            ..def_with_filter(KeywordFilterMode::None, &[])
        };

        let pops = vec![
            RawPop {
                name: "sto".to_string(),
                desc: "Sweden Stockholm".to_string(),
                relays: vec![RawRelay {
                    ipv4: "1.1.1.1".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27060,
                }],
            },
            RawPop {
                name: "got".to_string(),
                desc: "Sweden Gothenburg".to_string(),
                relays: vec![RawRelay {
                    ipv4: "2.2.2.2".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27060,
                }],
            },
            RawPop {
                name: "hkg".to_string(),
                desc: "Hong Kong".to_string(),
                relays: vec![RawRelay {
                    ipv4: "3.3.3.3".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27140,
                }],
            },
            RawPop {
                name: "tyo".to_string(),
                desc: "Japan Tokyo".to_string(),
                relays: vec![RawRelay {
                    ipv4: "4.4.4.4".to_string(),
                    port_range_min: 27015,
                    port_range_max: 27060,
                }],
            },
        ];

        let data = build_server_data("1", pops, &def);

        assert_eq!(data.unclustered_servers.len(), 4);
        assert_eq!(data.clustered_servers.len(), 3); // Sweden, Hong Kong, Japan

        let sweden = data
            .clustered_servers
            .iter()
            .find(|s| s.description == "Sweden")
            .expect("Sweden cluster should exist");
        assert_eq!(sweden.name, "cluster");
        assert_eq!(sweden.relays.len(), 2);

        let hong_kong = data
            .clustered_servers
            .iter()
            .find(|s| s.description == "Hong Kong")
            .expect("Hong Kong cluster should exist");
        assert_eq!(hong_kong.relays.len(), 1);

        let japan = data
            .clustered_servers
            .iter()
            .find(|s| s.description == "Japan Tokyo")
            .expect("Japan server should exist as itself");
        assert_eq!(japan.name, "tyo");
    }

    #[test]
    fn filter_is_case_insensitive() {
        let def = def_with_filter(KeywordFilterMode::Exclude, &["china"]);
        assert!(!is_server_accepted("CHINA East", &def));
        assert!(!is_server_accepted("China East", &def));
    }
}
