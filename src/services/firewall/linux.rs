use crate::core::server::Server;
use crate::services::firewall::{CommandRunner, FirewallError, FirewallService};

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
}

fn command_exists(command: &str) -> bool {
    std::process::Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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
                Ok(String::new())
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
}
