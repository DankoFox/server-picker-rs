use crate::core::server::Server;
use crate::services::firewall::{FirewallError, FirewallService};

/// Windows firewall placeholder for Phase 10.
pub struct WindowsFirewall;

impl WindowsFirewall {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl FirewallService for WindowsFirewall {
    async fn block_servers(&self, _servers: &[Server]) -> Result<(), FirewallError> {
        Err(FirewallError::Backend(
            "Windows firewall not yet implemented".to_string(),
        ))
    }

    async fn unblock_servers(&self, _servers: &[Server]) -> Result<(), FirewallError> {
        Err(FirewallError::Backend(
            "Windows firewall not yet implemented".to_string(),
        ))
    }

    async fn reset_firewall(&self) -> Result<(), FirewallError> {
        Err(FirewallError::Backend(
            "Windows firewall not yet implemented".to_string(),
        ))
    }
}
