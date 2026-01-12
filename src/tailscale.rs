use std::process::Command;
use tracing::{debug, info, warn};

/// Result type for Tailscale operations
pub type TailscaleResult<T> = Result<T, TailscaleError>;

/// Errors that can occur during Tailscale operations
#[derive(Debug)]
pub enum TailscaleError {
    CommandFailed(String),
    NotFound(String),
}

impl std::fmt::Display for TailscaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TailscaleError::CommandFailed(msg) => write!(f, "Command failed: {}", msg),
            TailscaleError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl std::error::Error for TailscaleError {}

/// Check if Tailscale is installed and available
pub fn is_tailscale_installed() -> bool {
    let result = Command::new("which").arg("tailscale").output();
    result.map(|output| output.status.success()).unwrap_or(false)
}

/// Execute `tailscale up` to enable Tailscale
pub fn tailscale_up() -> TailscaleResult<()> {
    if !is_tailscale_installed() {
        return Err(TailscaleError::NotFound("tailscale executable not found".to_string()));
    }

    info!("Executing: tailscale up");
    let output = Command::new("tailscale")
        .arg("up")
        .output()
        .map_err(|e| TailscaleError::CommandFailed(format!("Failed to execute tailscale up: {}", e)))?;

    if output.status.success() {
        debug!("tailscale up succeeded");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("tailscale up failed: {}", stderr);
        Err(TailscaleError::CommandFailed(stderr.to_string()))
    }
}

/// Execute `tailscale down` to disable Tailscale
pub fn tailscale_down() -> TailscaleResult<()> {
    if !is_tailscale_installed() {
        return Err(TailscaleError::NotFound("tailscale executable not found".to_string()));
    }

    info!("Executing: tailscale down");
    let output = Command::new("tailscale")
        .arg("down")
        .output()
        .map_err(|e| TailscaleError::CommandFailed(format!("Failed to execute tailscale down: {}", e)))?;

    if output.status.success() {
        debug!("tailscale down succeeded");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("tailscale down failed: {}", stderr);
        Err(TailscaleError::CommandFailed(stderr.to_string()))
    }
}

/// Check if Tailscale is currently running (connected)
pub fn is_tailscale_running() -> bool {
    if !is_tailscale_installed() {
        return false;
    }

    let output = Command::new("tailscale")
        .args(["status", "-json"])
        .output();

    match output {
        Ok(out) => {
            // Check if the command succeeded and contains backend state
            if out.status.success() {
                if let Ok(text) = std::str::from_utf8(&out.stdout) {
                    // Simple check: if we can parse it and it has a backend state
                    if let Ok(json) = json::parse(text) {
                        return json["BackendState"]
                            .as_str()
                            .map(|s| s == "Running")
                            .unwrap_or(false);
                    }
                }
            }
            false
        }
        Err(_) => false,
    }
}
