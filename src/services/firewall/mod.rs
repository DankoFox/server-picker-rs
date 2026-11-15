pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

use crate::core::server::Server;

/// Errors that can occur during firewall operations.
#[derive(Debug, thiserror::Error)]
pub enum FirewallError {
    #[error("Not running with root privileges")]
    NotRoot,
    #[error("No supported firewall backend found (nftables or iptables)")]
    NoBackend,
    #[error("Backend error: {0}")]
    Backend(String),
    #[error("Command '{command}' failed with exit code {exit_code}: {stderr}")]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait abstracting platform-specific firewall operations.
#[async_trait::async_trait]
pub trait FirewallService: Send + Sync {
    /// Block traffic to/from the given servers.
    async fn block_servers(&self, servers: &[Server]) -> Result<(), FirewallError>;

    /// Unblock traffic to/from the given servers.
    async fn unblock_servers(&self, servers: &[Server]) -> Result<(), FirewallError>;

    /// Remove all rules created by this application.
    async fn reset_firewall(&self) -> Result<(), FirewallError>;

    // TODO(Phase 11/polish): Add a query method such as
    // `async fn blocked_relays(&self) -> Result<HashSet<String>, FirewallError>`
    // so the UI can reflect the real firewall state instead of only tracking
    // in-session block/unblock actions. This will require parsing nftables set
    // members or iptables rules and may need privilege escalation.
}

/// Abstraction over process execution so backends can be unit tested.
#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    /// Run a command with the given arguments and return stdout on success.
    async fn run(&self, command: &str, args: &[String]) -> Result<String, FirewallError>;

    /// Run a shell script containing one or more commands. This is used by
    /// backends to batch firewall operations so privilege escalation happens
    /// only once per user action.
    async fn run_script(&self, script: &str) -> Result<String, FirewallError>;
}

/// Privilege escalation strategy for running firewall commands as root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeEscalation {
    /// Use `pkexec` (triggers the desktop authentication agent / polkit).
    Pkexec,
    /// Fall back to `sudo`.
    Sudo,
}

/// Run commands using the real OS shell, optionally elevated.
pub struct TokioCommandRunner {
    escalation: Option<PrivilegeEscalation>,
}

impl TokioCommandRunner {
    /// Run commands as the current user.
    pub fn new() -> Self {
        Self { escalation: None }
    }

    /// Run commands elevated, preferring `pkexec` over `sudo`.
    pub fn privileged() -> Self {
        Self {
            escalation: detect_privilege_escalation(),
        }
    }
}

impl Default for TokioCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl CommandRunner for TokioCommandRunner {
    async fn run(&self, command: &str, args: &[String]) -> Result<String, FirewallError> {
        let (cmd, owned_args): (String, Vec<String>) = match self.escalation {
            Some(PrivilegeEscalation::Pkexec) => {
                let mut all = vec![command.to_string()];
                all.extend(args.iter().cloned());
                ("pkexec".to_string(), all)
            }
            Some(PrivilegeEscalation::Sudo) => {
                let mut all = vec![command.to_string()];
                all.extend(args.iter().cloned());
                ("sudo".to_string(), all)
            }
            None => (command.to_string(), args.to_vec()),
        };

        Self::execute(&cmd, &owned_args).await
    }

    async fn run_script(&self, script: &str) -> Result<String, FirewallError> {
        let (cmd, args): (String, Vec<String>) = match self.escalation {
            Some(PrivilegeEscalation::Pkexec) => (
                "pkexec".to_string(),
                vec!["bash".to_string(), "-c".to_string(), script.to_string()],
            ),
            Some(PrivilegeEscalation::Sudo) => (
                "sudo".to_string(),
                vec!["bash".to_string(), "-c".to_string(), script.to_string()],
            ),
            None => (
                "bash".to_string(),
                vec!["-c".to_string(), script.to_string()],
            ),
        };

        Self::execute(&cmd, &args).await
    }
}

impl TokioCommandRunner {
    async fn execute(cmd: &str, args: &[String]) -> Result<String, FirewallError> {
        let output = tokio::process::Command::new(cmd)
            .args(args)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(FirewallError::CommandFailed {
                command: format!("{} {}", cmd, args.join(" ")),
                exit_code: output.status.code().unwrap_or(-1),
                stderr,
            });
        }

        Ok(stdout)
    }
}

fn detect_privilege_escalation() -> Option<PrivilegeEscalation> {
    if command_exists("pkexec") {
        Some(PrivilegeEscalation::Pkexec)
    } else if command_exists("sudo") {
        Some(PrivilegeEscalation::Sudo)
    } else {
        None
    }
}

fn command_exists(command: &str) -> bool {
    std::process::Command::new("which")
        .arg(command)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Returns `true` if the current process is running as root.
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::getuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Request root privileges by relaunching the current executable under
/// `pkexec` or `sudo`. This is a no-op on Windows.
///
/// # Safety
/// This replaces the current process; it does not return on success.
pub fn relaunch_as_root() -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        let command = if std::process::Command::new("pkexec")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            "pkexec"
        } else {
            "sudo"
        };

        let status = std::process::Command::new(command)
            .arg(&exe)
            .args(args)
            .status()?;

        if status.success() {
            std::process::exit(0);
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Failed to obtain root privileges",
            ))
        }
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// A no-op firewall for development or when no backend is available.
pub struct NoopFirewall;

#[async_trait::async_trait]
impl FirewallService for NoopFirewall {
    async fn block_servers(
        &self,
        _servers: &[crate::core::server::Server],
    ) -> Result<(), FirewallError> {
        Ok(())
    }

    async fn unblock_servers(
        &self,
        _servers: &[crate::core::server::Server],
    ) -> Result<(), FirewallError> {
        Ok(())
    }

    async fn reset_firewall(&self) -> Result<(), FirewallError> {
        Ok(())
    }
}

/// Create a firewall service appropriate for the current platform.
/// On Linux, if the process is not root, commands are wrapped with `pkexec`
/// (preferred) or `sudo` so the desktop polkit authentication agent pops up
/// only when a firewall operation is requested.
pub async fn create_firewall_service() -> Result<std::sync::Arc<dyn FirewallService>, FirewallError>
{
    #[cfg(target_os = "linux")]
    {
        let runner = if is_root() {
            TokioCommandRunner::new()
        } else {
            TokioCommandRunner::privileged()
        };

        match crate::services::firewall::linux::LinuxFirewall::detect(runner).await {
            Ok(fw) => Ok(std::sync::Arc::new(fw)),
            Err(_) => {
                tracing::warn!("No firewall backend found; using no-op firewall");
                Ok(std::sync::Arc::new(NoopFirewall))
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        Ok(std::sync::Arc::new(
            crate::services::firewall::windows::WindowsFirewall::new(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(std::sync::Arc::new(NoopFirewall))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_root_does_not_panic() {
        // Just ensure the call is safe on the current platform.
        let _ = is_root();
    }
}
