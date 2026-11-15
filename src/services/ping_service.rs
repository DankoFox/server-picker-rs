use crate::core::server::{Server, ServerStatus};

#[cfg(test)]
use crate::core::server::Relay;
use std::net::IpAddr;
use std::time::Duration;
use surge_ping::{Client, Config, PingIdentifier, PingSequence, SurgeError};
use tokio::net::TcpStream;
use tokio::time::{timeout, Instant};

const ICMP_TIMEOUT_MS: u64 = 500;
const PROBE_TIMEOUT_MS: u64 = 1000;
const PROBE_COUNT: usize = 2;
const TCP_TIMEOUT_MS: u64 = 1000;

/// Errors that can occur during ping operations.
#[derive(Debug, thiserror::Error)]
pub enum PingError {
    #[error("ICMP client initialization failed: {0}")]
    IcmpInit(#[from] SurgeError),
    #[error("Invalid IP address: {0}")]
    InvalidIp(String),
}

/// Measures server reachability using ICMP with a TCP-connect fallback.
pub struct PingService {
    icmp_timeout: Duration,
    probe_timeout: Duration,
    probe_count: usize,
    tcp_timeout: Duration,
}

impl Default for PingService {
    fn default() -> Self {
        Self::new()
    }
}

impl PingService {
    /// Create a new ping service with default timeouts.
    pub fn new() -> Self {
        Self {
            icmp_timeout: Duration::from_millis(ICMP_TIMEOUT_MS),
            probe_timeout: Duration::from_millis(PROBE_TIMEOUT_MS),
            probe_count: PROBE_COUNT,
            tcp_timeout: Duration::from_millis(TCP_TIMEOUT_MS),
        }
    }

    /// Create a ping service with custom timeouts (useful for tests).
    pub fn with_timeouts(
        icmp_timeout: Duration,
        probe_timeout: Duration,
        probe_count: usize,
        tcp_timeout: Duration,
    ) -> Self {
        Self {
            icmp_timeout,
            probe_timeout,
            probe_count,
            tcp_timeout,
        }
    }

    /// Ping a single server, updating its measurement fields in place.
    pub async fn ping_server(&self, server: &mut Server) {
        if server.relays.is_empty() {
            tracing::debug!(server = %server.name, "no relays to ping");
            server.status = ServerStatus::Failed;
            return;
        }

        let icmp_client = Client::new(&Config::default()).ok();
        if icmp_client.is_none() {
            tracing::debug!(server = %server.name, "failed to create ICMP client; will use TCP fallback");
        }

        server.clear_measurement();
        server.status = ServerStatus::Pinging;

        tracing::debug!(server = %server.name, relays = ?server.relays.iter().map(|r| &r.ipv4).collect::<Vec<_>>(), "starting relay scan");

        let mut best_rtt = Duration::MAX;
        let mut best_relay: Option<(String, u16)> = None;

        // Phase 1: find the relay with the lowest RTT.
        for relay in &server.relays {
            if let Some(rtt) = self
                .measure_relay(&relay.ipv4, relay.port_range_min, &icmp_client)
                .await
            {
                tracing::debug!(server = %server.name, relay = %relay.ipv4, rtt_ms = %rtt.as_millis(), "relay measured");
                if rtt < best_rtt {
                    best_rtt = rtt;
                    best_relay = Some((relay.ipv4.clone(), relay.port_range_min));
                }
            } else {
                tracing::debug!(server = %server.name, relay = %relay.ipv4, "relay unreachable");
            }
        }

        let Some((best_ipv4, best_port)) = best_relay else {
            tracing::debug!(server = %server.name, "all relays unreachable");
            server.status = ServerStatus::Failed;
            return;
        };

        tracing::debug!(server = %server.name, best_relay = %best_ipv4, "selected best relay");

        // Phase 2: probe the best relay several times for packet loss.
        server.status = ServerStatus::Probing;
        let mut successes = 0u32;
        let mut final_best_rtt = Duration::MAX;

        for _ in 0..self.probe_count {
            if let Some(rtt) = self
                .measure_relay_with_timeout(&best_ipv4, best_port, self.probe_timeout, &icmp_client)
                .await
            {
                successes += 1;
                final_best_rtt = final_best_rtt.min(rtt);
            }
        }

        if successes > 0 {
            server.push_ping_sample(final_best_rtt.as_millis() as u32);
            server.status = ServerStatus::Ok;
            server.packet_loss_percent = Some(
                (((self.probe_count - successes as usize) as f64 / self.probe_count as f64) * 100.0)
                    as u8,
            );
        } else {
            // Keep existing history/average on failure so the UI stays stable.
            server.status = ServerStatus::Failed;
        }
    }

    /// Ping all servers concurrently.
    pub async fn ping_servers(&self, servers: &mut [Server]) {
        let futures: Vec<_> = servers
            .iter_mut()
            .map(|server| self.ping_server(server))
            .collect();
        futures::future::join_all(futures).await;
    }

    /// Measure a single relay using ICMP, falling back to a TCP connect.
    async fn measure_relay(
        &self,
        ipv4: &str,
        port: u16,
        icmp_client: &Option<Client>,
    ) -> Option<Duration> {
        self.measure_relay_with_timeout(ipv4, port, self.icmp_timeout, icmp_client)
            .await
    }

    async fn measure_relay_with_timeout(
        &self,
        ipv4: &str,
        port: u16,
        icmp_timeout: Duration,
        icmp_client: &Option<Client>,
    ) -> Option<Duration> {
        // Try ICMP first if we have a client.
        if let Some(client) = icmp_client {
            if let Ok(addr) = ipv4.parse::<IpAddr>() {
                let mut pinger = client.pinger(addr, PingIdentifier(0)).await;
                let sequence = PingSequence(0);
                let payload = [];

                if let Ok(Ok((_, rtt))) =
                    timeout(icmp_timeout, pinger.ping(sequence, &payload)).await
                {
                    tracing::debug!(relay = %ipv4, rtt_ms = %rtt.as_millis(), "ICMP success");
                    return Some(rtt);
                }
                tracing::debug!(relay = %ipv4, "ICMP failed, falling back to TCP");
            }
        }

        // Fall back to TCP connect on the first port in the relay's range.
        let rtt = self.tcp_connect(ipv4, port).await;
        if rtt.is_some() {
            tracing::debug!(relay = %ipv4, port = port, "TCP fallback success");
        } else {
            tracing::debug!(relay = %ipv4, port = port, "TCP fallback failed");
        }
        rtt
    }

    async fn tcp_connect(&self, host: &str, port: u16) -> Option<Duration> {
        let addr = format!("{}:{}", host, port);
        let start = Instant::now();

        match timeout(self.tcp_timeout, TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => Some(start.elapsed()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_service_can_be_created() {
        let _service = PingService::new();
    }

    #[tokio::test]
    async fn ping_server_with_no_relays_fails() {
        let service = PingService::new();
        let mut server = Server::default();
        service.ping_server(&mut server).await;
        assert_eq!(server.status, ServerStatus::Failed);
    }

    #[tokio::test]
    async fn ping_server_with_unresolvable_relay_fails() {
        let service = PingService::new();
        let mut server = Server::new("test", "Test");
        server.relays.push(Relay::new("invalid.ip.address.here"));
        service.ping_server(&mut server).await;
        assert_eq!(server.status, ServerStatus::Failed);
    }
}
