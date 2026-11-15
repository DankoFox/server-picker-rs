use crate::core::server::Server;
use crate::services::firewall::{CommandRunner, FirewallError, FirewallService};
use std::collections::HashSet;

const NFTABLES_TABLE: &str = "server_picker_x";
const NFTABLES_SET: &str = "blocked_ips";
const IPTABLES_CHAIN: &str = "SERVER_PICKER_X";

/// Supported Linux firewall backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxFirewallBackend {
    /// Modern `nftables` backend (preferred).
    Nftables,
    /// Legacy `iptables` fallback.
    Iptables,
}

/// Linux firewall implementation that shells out to `nftables` or `iptables`.
pub struct LinuxFirewall<R: CommandRunner> {
    backend: LinuxFirewallBackend,
    runner: R,
}

impl<R: CommandRunner> LinuxFirewall<R> {
    /// Create a firewall service using a specific backend.
    pub fn with_backend(backend: LinuxFirewallBackend, runner: R) -> Self {
        Self { backend, runner }
    }

    /// Detect the best available backend.
    pub async fn detect(runner: R) -> Result<Self, FirewallError> {
        if command_exists("nft") {
            return Ok(Self::with_backend(LinuxFirewallBackend::Nftables, runner));
        }

        if command_exists("iptables") {
            return Ok(Self::with_backend(LinuxFirewallBackend::Iptables, runner));
        }

        Err(FirewallError::NoBackend)
    }

    fn collect_ips(servers: &[Server]) -> Vec<String> {
        servers
            .iter()
            .flat_map(|server| server.relays.iter().map(|relay| relay.ipv4.clone()))
            .filter(|ip| !ip.is_empty())
            .collect()
    }

    fn format_nft_set_elements(ips: &[String]) -> String {
        format!("{{ {} }}", ips.join(", "))
    }
}

#[async_trait::async_trait]
impl<R: CommandRunner> FirewallService for LinuxFirewall<R> {
    async fn block_servers(&self, servers: &[Server]) -> Result<(), FirewallError> {
        match self.backend {
            LinuxFirewallBackend::Nftables => self.block_nftables(servers).await,
            LinuxFirewallBackend::Iptables => self.block_iptables(servers).await,
        }
    }

    async fn unblock_servers(&self, servers: &[Server]) -> Result<(), FirewallError> {
        match self.backend {
            LinuxFirewallBackend::Nftables => self.unblock_nftables(servers).await,
            LinuxFirewallBackend::Iptables => self.unblock_iptables(servers).await,
        }
    }

    async fn reset_firewall(&self) -> Result<(), FirewallError> {
        match self.backend {
            LinuxFirewallBackend::Nftables => self.reset_nftables().await,
            LinuxFirewallBackend::Iptables => self.reset_iptables().await,
        }
    }

    async fn reset_and_block_servers(&self, servers: &[Server]) -> Result<(), FirewallError> {
        match self.backend {
            LinuxFirewallBackend::Nftables => self.reset_and_block_nftables(servers).await,
            LinuxFirewallBackend::Iptables => self.reset_and_block_iptables(servers).await,
        }
    }

    async fn blocked_relays(&self) -> Result<HashSet<String>, FirewallError> {
        match self.backend {
            LinuxFirewallBackend::Nftables => self.list_blocked_nftables().await,
            LinuxFirewallBackend::Iptables => self.list_blocked_iptables().await,
        }
    }
}

impl<R: CommandRunner> LinuxFirewall<R> {
    // ------------------------------------------------------------------
    // nftables implementation
    // ------------------------------------------------------------------

    async fn block_nftables(&self, servers: &[Server]) -> Result<(), FirewallError> {
        let ips = Self::collect_ips(servers);
        if ips.is_empty() {
            return Ok(());
        }

        let elements = Self::format_nft_set_elements(&ips);
        let nft_script = format!(
            "add table inet {table}\n\
             add chain inet {table} input {{ type filter hook input priority 0 ; policy accept ; }}\n\
             add set inet {table} {set} {{ type ipv4_addr ; flags interval ; }}\n\
             add rule inet {table} input ip saddr @{set} drop\n\
             add element inet {table} {set} {elements}\n",
            table = NFTABLES_TABLE,
            set = NFTABLES_SET,
        );

        self.run_nft_script(&nft_script).await.map(|_| ())
    }

    async fn unblock_nftables(&self, servers: &[Server]) -> Result<(), FirewallError> {
        let ips = Self::collect_ips(servers);
        if ips.is_empty() {
            return Ok(());
        }

        let elements = Self::format_nft_set_elements(&ips);
        let nft_script = format!(
            "delete element inet {table} {set} {elements}\n",
            table = NFTABLES_TABLE,
            set = NFTABLES_SET,
        );

        // nft fails if the table or element does not exist; ignore that.
        self.run_nft_script(&nft_script).await.ok();
        Ok(())
    }

    async fn reset_nftables(&self) -> Result<(), FirewallError> {
        let nft_script = format!("delete table inet {}\n", NFTABLES_TABLE);
        self.run_nft_script(&nft_script).await.ok();
        Ok(())
    }

    async fn reset_and_block_nftables(&self, servers: &[Server]) -> Result<(), FirewallError> {
        let ips = Self::collect_ips(servers);
        if ips.is_empty() {
            return self.reset_nftables().await;
        }

        let elements = Self::format_nft_set_elements(&ips);
        let nft_script = format!(
            "delete table inet {table}\n\
             add table inet {table}\n\
             add chain inet {table} input {{ type filter hook input priority 0 ; policy accept ; }}\n\
             add set inet {table} {set} {{ type ipv4_addr ; flags interval ; }}\n\
             add rule inet {table} input ip saddr @{set} drop\n\
             add element inet {table} {set} {elements}\n",
            table = NFTABLES_TABLE,
            set = NFTABLES_SET,
        );

        self.run_nft_script(&nft_script).await.map(|_| ())
    }

    async fn run_nft_script(&self, nft_script: &str) -> Result<String, FirewallError> {
        let shell_script = format!("nft -f - <<'EOF'\n{}EOF\n", nft_script);
        self.runner.run_script(&shell_script).await
    }

    // ------------------------------------------------------------------
    // iptables implementation
    // ------------------------------------------------------------------

    async fn block_iptables(&self, servers: &[Server]) -> Result<(), FirewallError> {
        let ips = Self::collect_ips(servers);
        if ips.is_empty() {
            return Ok(());
        }

        let mut script = format!(
            "iptables -L {chain} >/dev/null 2>&1 || iptables -N {chain}; \
             iptables -C INPUT -j {chain} >/dev/null 2>&1 || iptables -A INPUT -j {chain}",
            chain = IPTABLES_CHAIN,
        );

        for ip in ips {
            script.push_str(&format!(
                "; iptables -A {chain} -s {ip} -j DROP",
                chain = IPTABLES_CHAIN,
                ip = ip
            ));
        }

        self.runner.run_script(&script).await?;
        Ok(())
    }

    async fn unblock_iptables(&self, servers: &[Server]) -> Result<(), FirewallError> {
        let ips = Self::collect_ips(servers);
        if ips.is_empty() {
            return Ok(());
        }

        let mut script = String::new();
        for ip in ips {
            if !script.is_empty() {
                script.push_str("; ");
            }
            script.push_str(&format!(
                "iptables -D {chain} -s {ip} -j DROP",
                chain = IPTABLES_CHAIN,
                ip = ip
            ));
        }

        self.runner.run_script(&script).await.ok();
        Ok(())
    }

    async fn reset_iptables(&self) -> Result<(), FirewallError> {
        let script = format!(
            "iptables -F {chain}; iptables -D INPUT -j {chain}; iptables -X {chain}",
            chain = IPTABLES_CHAIN,
        );
        self.runner.run_script(&script).await.ok();
        Ok(())
    }

    async fn reset_and_block_iptables(&self, servers: &[Server]) -> Result<(), FirewallError> {
        let ips = Self::collect_ips(servers);
        let mut script = format!(
            "iptables -F {chain} 2>/dev/null; iptables -D INPUT -j {chain} 2>/dev/null; iptables -X {chain} 2>/dev/null; \
             iptables -N {chain}; iptables -A INPUT -j {chain}",
            chain = IPTABLES_CHAIN,
        );

        for ip in ips {
            script.push_str(&format!(
                "; iptables -A {chain} -s {ip} -j DROP",
                chain = IPTABLES_CHAIN,
                ip = ip
            ));
        }

        self.runner.run_script(&script).await?;
        Ok(())
    }

    async fn list_blocked_nftables(&self) -> Result<HashSet<String>, FirewallError> {
        let output = self
            .runner
            .run(
                "nft",
                &[
                    "list".to_string(),
                    "set".to_string(),
                    "inet".to_string(),
                    NFTABLES_TABLE.to_string(),
                    NFTABLES_SET.to_string(),
                ],
            )
            .await;

        match output {
            Ok(stdout) => Ok(parse_nft_set_elements(&stdout)),
            // If the table or set does not exist, nothing is blocked.
            Err(_) => Ok(HashSet::new()),
        }
    }

    async fn list_blocked_iptables(&self) -> Result<HashSet<String>, FirewallError> {
        let output = self
            .runner
            .run("iptables", &["-S".to_string(), IPTABLES_CHAIN.to_string()])
            .await;

        match output {
            Ok(stdout) => Ok(parse_iptables_rules(&stdout)),
            // If the chain does not exist, nothing is blocked.
            Err(_) => Ok(HashSet::new()),
        }
    }
}

fn command_exists(command: &str) -> bool {
    std::process::Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Normalize an IP returned by a firewall backend so it matches the exact
/// relay IPv4 strings stored in `Server::relays`. In practice the app only
/// blocks individual host addresses, so /32 prefixes are stripped.
fn normalize_blocked_ip(ip: &str) -> String {
    ip.strip_suffix("/32").unwrap_or(ip).to_string()
}

/// Parse `nft list set` output and return the IPv4 addresses in the set.
///
/// Handles both single-line and multi-line `elements = { ... }` blocks.
fn parse_nft_set_elements(output: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    let mut collecting = false;
    let mut brace_depth: usize = 0;
    let mut buffer = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if !collecting {
            if let Some(start) = trimmed.find("elements = {") {
                collecting = true;
                let content = &trimmed[start + "elements = {".len()..];
                brace_depth = 1 + content.matches('{').count();
                brace_depth = brace_depth.saturating_sub(content.matches('}').count());
                buffer.push_str(content);
            } else {
                continue;
            }
        } else {
            brace_depth += trimmed.matches('{').count();
            brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());
            buffer.push(' ');
            buffer.push_str(trimmed);
        }

        if brace_depth == 0 {
            let content = buffer.replace(['{', '}'], "");
            for item in content.split(',') {
                let ip = item.trim();
                if !ip.is_empty() {
                    result.insert(normalize_blocked_ip(ip));
                }
            }
            break;
        }
    }

    result
}

/// Parse `iptables -S <chain>` output and return source IPv4 addresses from
/// DROP rules.
fn parse_iptables_rules(output: &str) -> HashSet<String> {
    let mut result = HashSet::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let Some(pos) = parts.iter().position(|&p| p == "-s") else {
            continue;
        };
        if let Some(ip) = parts.get(pos + 1) {
            result.insert(normalize_blocked_ip(ip));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::server::{Relay, Server};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MockRunner {
        commands: Arc<Mutex<Vec<(String, Vec<String>)>>>,
        scripts: Arc<Mutex<Vec<String>>>,
        succeed: bool,
        output: String,
    }

    impl MockRunner {
        fn commands(&self) -> Vec<(String, Vec<String>)> {
            self.commands.lock().unwrap().clone()
        }

        fn scripts(&self) -> Vec<String> {
            self.scripts.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl CommandRunner for MockRunner {
        async fn run(&self, command: &str, args: &[String]) -> Result<String, FirewallError> {
            self.commands
                .lock()
                .unwrap()
                .push((command.to_string(), args.to_vec()));
            if self.succeed {
                Ok(self.output.clone())
            } else {
                Err(FirewallError::CommandFailed {
                    command: command.to_string(),
                    exit_code: 1,
                    stderr: "mock error".to_string(),
                })
            }
        }

        async fn run_script(&self, script: &str) -> Result<String, FirewallError> {
            self.scripts.lock().unwrap().push(script.to_string());
            if self.succeed {
                Ok(String::new())
            } else {
                Err(FirewallError::CommandFailed {
                    command: "bash".to_string(),
                    exit_code: 1,
                    stderr: "mock error".to_string(),
                })
            }
        }
    }

    fn sample_servers() -> Vec<Server> {
        vec![
            Server {
                name: "sto".to_string(),
                description: "Sweden".to_string(),
                relays: vec![Relay::new("192.0.2.1"), Relay::new("192.0.2.2")],
                ..Server::default()
            },
            Server {
                name: "hkg".to_string(),
                description: "Hong Kong".to_string(),
                relays: vec![Relay::new("198.51.100.1")],
                ..Server::default()
            },
        ]
    }

    #[tokio::test]
    async fn nftables_block_builds_expected_script() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Nftables, runner.clone());
        fw.block_servers(&sample_servers()).await.unwrap();

        let scripts = runner.scripts();
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        assert!(script.contains("add table inet server_picker_x"));
        assert!(script.contains("add element inet server_picker_x blocked_ips"));
        assert!(script.contains("192.0.2.1"));
        assert!(script.contains("198.51.100.1"));
    }

    #[tokio::test]
    async fn nftables_reset_deletes_table() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Nftables, runner.clone());
        fw.reset_firewall().await.unwrap();

        let scripts = runner.scripts();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("delete table inet server_picker_x"));
    }

    #[tokio::test]
    async fn nftables_reset_and_block_builds_single_script() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Nftables, runner.clone());
        fw.reset_and_block_servers(&sample_servers()).await.unwrap();

        let scripts = runner.scripts();
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        assert!(script.contains("delete table inet server_picker_x"));
        assert!(script.contains("add table inet server_picker_x"));
        assert!(script.contains("192.0.2.1"));
        assert!(script.contains("198.51.100.1"));
    }

    #[tokio::test]
    async fn iptables_block_adds_drop_rules() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Iptables, runner.clone());
        fw.block_servers(&sample_servers()).await.unwrap();

        let scripts = runner.scripts();
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        assert!(script.contains("iptables -A SERVER_PICKER_X -s 192.0.2.1 -j DROP"));
        assert!(script.contains("iptables -A SERVER_PICKER_X -s 198.51.100.1 -j DROP"));
    }

    #[tokio::test]
    async fn iptables_reset_removes_chain_and_jump() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Iptables, runner.clone());
        fw.reset_firewall().await.unwrap();

        let scripts = runner.scripts();
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        assert!(script.contains("iptables -F SERVER_PICKER_X"));
        assert!(script.contains("iptables -D INPUT -j SERVER_PICKER_X"));
        assert!(script.contains("iptables -X SERVER_PICKER_X"));
    }

    #[tokio::test]
    async fn iptables_reset_and_block_builds_single_script() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Iptables, runner.clone());
        fw.reset_and_block_servers(&sample_servers()).await.unwrap();

        let scripts = runner.scripts();
        assert_eq!(scripts.len(), 1);
        let script = &scripts[0];
        assert!(script.contains("iptables -F SERVER_PICKER_X"));
        assert!(script.contains("iptables -A INPUT -j SERVER_PICKER_X"));
        assert!(script.contains("iptables -A SERVER_PICKER_X -s 192.0.2.1 -j DROP"));
        assert!(script.contains("iptables -A SERVER_PICKER_X -s 198.51.100.1 -j DROP"));
    }

    #[tokio::test]
    async fn empty_server_list_is_no_op() {
        let runner = MockRunner {
            succeed: true,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Nftables, runner.clone());
        fw.block_servers(&[]).await.unwrap();
        assert!(runner.scripts().is_empty());
        assert!(runner.commands().is_empty());
    }

    #[tokio::test]
    async fn nftables_blocked_relays_returns_set_members() {
        let runner = MockRunner {
            succeed: true,
            output: "elements = { 198.51.100.1 }".to_string(),
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Nftables, runner.clone());
        let relays = fw.blocked_relays().await.unwrap();
        assert!(relays.contains("198.51.100.1"));
        assert_eq!(relays.len(), 1);

        let commands = runner.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "nft");
        assert!(commands[0].1.contains(&"list".to_string()));
        assert!(commands[0].1.contains(&"blocked_ips".to_string()));
    }

    #[tokio::test]
    async fn nftables_blocked_relays_returns_empty_on_missing_table() {
        let runner = MockRunner {
            succeed: false,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Nftables, runner.clone());
        let relays = fw.blocked_relays().await.unwrap();
        assert!(relays.is_empty());
    }

    #[tokio::test]
    async fn iptables_blocked_relays_returns_source_ips() {
        let runner = MockRunner {
            succeed: true,
            output: "-N SERVER_PICKER_X\n-A SERVER_PICKER_X -s 198.51.100.1/32 -j DROP".to_string(),
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Iptables, runner.clone());
        let relays = fw.blocked_relays().await.unwrap();
        assert!(relays.contains("198.51.100.1"));
        assert_eq!(relays.len(), 1);

        let commands = runner.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "iptables");
        assert!(commands[0].1.contains(&"-S".to_string()));
        assert!(commands[0].1.contains(&"SERVER_PICKER_X".to_string()));
    }

    #[tokio::test]
    async fn iptables_blocked_relays_returns_empty_on_missing_chain() {
        let runner = MockRunner {
            succeed: false,
            ..Default::default()
        };
        let fw = LinuxFirewall::with_backend(LinuxFirewallBackend::Iptables, runner.clone());
        let relays = fw.blocked_relays().await.unwrap();
        assert!(relays.is_empty());
    }

    #[test]
    fn parse_nft_set_elements_extracts_ips() {
        let output = "table inet server_picker_x {\n\
            set blocked_ips {\n\
                type ipv4_addr\n\
                flags interval\n\
                elements = { 198.51.100.1, 203.0.113.0/24 }\n\
            }\n\
        }";
        let ips = parse_nft_set_elements(output);
        assert!(ips.contains("198.51.100.1"));
        assert!(ips.contains("203.0.113.0/24"));
        assert_eq!(ips.len(), 2);
    }

    #[test]
    fn parse_nft_set_elements_normalizes_host_prefix() {
        let output = "elements = { 198.51.100.1/32 }";
        let ips = parse_nft_set_elements(output);
        assert!(ips.contains("198.51.100.1"));
        assert!(!ips.contains("198.51.100.1/32"));
    }

    #[test]
    fn parse_nft_set_elements_handles_empty_set() {
        let output = "elements = { }";
        let ips = parse_nft_set_elements(output);
        assert!(ips.is_empty());
    }

    #[test]
    fn parse_nft_set_elements_handles_multiline_block() {
        let output = "set blocked_ips {\n\
            elements = {\n\
                198.51.100.1,\n\
                203.0.113.0/24\n\
            }\n\
        }";
        let ips = parse_nft_set_elements(output);
        assert!(ips.contains("198.51.100.1"));
        assert!(ips.contains("203.0.113.0/24"));
        assert_eq!(ips.len(), 2);
    }

    #[test]
    fn parse_iptables_rules_extracts_sources() {
        let output = "-N SERVER_PICKER_X\n\
            -A SERVER_PICKER_X -s 198.51.100.1/32 -j DROP\n\
            -A SERVER_PICKER_X -s 203.0.113.0/24 -j DROP";
        let ips = parse_iptables_rules(output);
        assert!(ips.contains("198.51.100.1"));
        assert!(ips.contains("203.0.113.0/24"));
        assert_eq!(ips.len(), 2);
    }

    #[test]
    fn parse_iptables_rules_ignores_non_drop_lines() {
        let output = "-N SERVER_PICKER_X\n\
            -A SERVER_PICKER_X -s 198.51.100.1/32 -j ACCEPT";
        let ips = parse_iptables_rules(output);
        assert!(ips.contains("198.51.100.1"));
    }
}
