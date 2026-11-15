use std::collections::VecDeque;

const MAX_PING_HISTORY: usize = 10;

/// A single relay entry for a server.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relay {
    pub ipv4: String,
    pub port_range_min: u16,
    pub port_range_max: u16,
}

impl Relay {
    pub fn new(ipv4: impl Into<String>) -> Self {
        Self {
            ipv4: ipv4.into(),
            port_range_min: 27015,
            port_range_max: 27015,
        }
    }

    pub fn with_port_range(
        ipv4: impl Into<String>,
        port_range_min: u16,
        port_range_max: u16,
    ) -> Self {
        Self {
            ipv4: ipv4.into(),
            port_range_min,
            port_range_max,
        }
    }
}

/// Health/status of a server as measured by ping probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServerStatus {
    /// No measurement has been taken yet.
    #[default]
    Unknown,
    /// A ping measurement is in progress.
    Pinging,
    /// Packet-loss probing of the best relay is in progress.
    Probing,
    /// The server is reachable.
    Ok,
    /// The server is unreachable or all probes timed out.
    Failed,
}

/// A displayable server (either a raw Valve POP or a cluster).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Server {
    /// Two-letter country/region code used as a compact flag representation.
    pub flag: String,
    /// Valve POP code, or `"cluster"` for merged clusters.
    pub name: String,
    /// Human-readable description. For clusters this is the cluster keyword.
    pub description: String,
    /// Relay IPv4 addresses belonging to this server/cluster.
    pub relays: Vec<Relay>,
    /// Best measured round-trip time in milliseconds.
    pub ping_ms: Option<u32>,
    /// Measured packet loss percentage (0-100).
    pub packet_loss_percent: Option<u8>,
    /// Reachability status.
    pub status: ServerStatus,
    /// Rolling window of recent ping measurements in milliseconds.
    pub ping_history: VecDeque<u32>,
}

impl Server {
    /// Create a new server with the given identity.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            ..Self::default()
        }
    }

    /// Map a server description (e.g. "Amsterdam (Netherlands)", "Hong Kong")
    /// to a two-letter country/region code.
    ///
    /// Valve descriptions usually put the country in parentheses at the end.
    /// When there are no parentheses, the whole string is treated as a region
    /// name (e.g. "Singapore", "Hong Kong", "Guam").
    pub fn country_code(description: &str) -> &'static str {
        let lower = description.trim().to_lowercase();

        // Extract country/region from the last pair of parentheses if present.
        let region = lower
            .rsplit_once('(')
            .and_then(|(_, after)| after.split_once(')').map(|(inside, _)| inside))
            .unwrap_or(&lower);

        match region.trim() {
            "argentina" => "AR",
            "australia" => "AU",
            "austria" => "AT",
            "belgium" => "BE",
            "brazil" => "BR",
            "bulgaria" => "BG",
            "canada" => "CA",
            "chile" => "CL",
            "china" => "CN",
            "czech republic" | "czechia" => "CZ",
            "denmark" => "DK",
            "england" => "GB",
            "finland" => "FI",
            "france" => "FR",
            "germany" => "DE",
            "guam" => "GU",
            "hong kong" => "HK",
            "india" => "IN",
            "italy" => "IT",
            "japan" => "JP",
            "mexico" => "MX",
            "netherlands" => "NL",
            "new zealand" => "NZ",
            "norway" => "NO",
            "peru" => "PE",
            "poland" => "PL",
            "puerto rico" => "PR",
            "romania" => "RO",
            "russia" => "RU",
            "saudi arabia" => "SA",
            "serbia" => "RS",
            "singapore" => "SG",
            "south africa" => "ZA",
            "south korea" => "KR",
            "spain" => "ES",
            "sri lanka" => "LK",
            "sweden" => "SE",
            "switzerland" => "CH",
            "thailand" => "TH",
            "turkey" => "TR",
            "uae" | "united arab emirates" => "AE",
            "uk" | "united kingdom" => "GB",
            "ukraine" => "UA",
            "usa" | "united states" => "US",
            "vietnam" => "VN",
            // US states / territories that appear in Valve descriptions.
            "california" | "georgia" | "illinois" | "texas" | "virginia" | "washington" => "US",
            _ => "??",
        }
    }

    /// Reset measurement fields.
    pub fn clear_measurement(&mut self) {
        self.ping_ms = None;
        self.packet_loss_percent = None;
        self.status = ServerStatus::Unknown;
    }

    /// Compute the average ping from the rolling history.
    pub fn average_ping_ms(&self) -> Option<u32> {
        if self.ping_history.is_empty() {
            return None;
        }
        let sum: u64 = self.ping_history.iter().map(|&ms| u64::from(ms)).sum();
        Some((sum / self.ping_history.len() as u64) as u32)
    }

    /// Push a new ping sample into the rolling history and update `ping_ms`
    /// to the rolling average.
    pub fn push_ping_sample(&mut self, ping_ms: u32) {
        self.ping_history.push_back(ping_ms);
        if self.ping_history.len() > MAX_PING_HISTORY {
            self.ping_history.pop_front();
        }
        self.ping_ms = self.average_ping_ms();
    }
}

/// Result of parsing the Valve SDR config for a game mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerData {
    /// Revision string returned by the Valve API.
    pub revision: String,
    /// Servers before any clustering is applied.
    pub unclustered_servers: Vec<Server>,
    /// Servers after cluster merging has been applied.
    pub clustered_servers: Vec<Server>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_stores_ipv4() {
        let relay = Relay::new("1.2.3.4");
        assert_eq!(relay.ipv4, "1.2.3.4");
    }

    #[test]
    fn server_clears_measurements() {
        let mut server = Server {
            ping_ms: Some(42),
            packet_loss_percent: Some(5),
            status: ServerStatus::Ok,
            ..Server::default()
        };
        server.clear_measurement();
        assert_eq!(server.ping_ms, None);
        assert_eq!(server.packet_loss_percent, None);
        assert_eq!(server.status, ServerStatus::Unknown);
    }

    #[test]
    fn country_code_maps_common_descriptions() {
        assert_eq!(Server::country_code("Amsterdam (Netherlands)"), "NL");
        assert_eq!(Server::country_code("Stockholm - Kista (Sweden)"), "SE");
        assert_eq!(Server::country_code("Tokyo Koto City (Japan)"), "JP");
        assert_eq!(Server::country_code("Dallas (Texas)"), "US");
        assert_eq!(Server::country_code("Hong Kong"), "HK");
        assert_eq!(Server::country_code("Singapore"), "SG");
        assert_eq!(Server::country_code("Guam"), "GU");
        assert_eq!(Server::country_code("Unknown Place"), "??");
    }
}
