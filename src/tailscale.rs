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
    debug!("Checking if Tailscale is installed");
    let result = Command::new("which").arg("tailscale").output();
    let installed = result.map(|output| output.status.success()).unwrap_or(false);
    debug!("Tailscale installed: {}", installed);
    installed
}

/// Execute `tailscale up` to enable Tailscale
pub fn tailscale_up() -> TailscaleResult<()> {
    info!("→ Enabling Tailscale (executing: tailscale up)");

    if !is_tailscale_installed() {
        warn!("Tailscale executable not found in PATH");
        return Err(TailscaleError::NotFound("tailscale executable not found".to_string()));
    }

    let output = Command::new("tailscale")
        .arg("up")
        .output()
        .map_err(|e| {
            warn!("Failed to execute tailscale up command: {}", e);
            TailscaleError::CommandFailed(format!("Failed to execute tailscale up: {}", e))
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("✓ Tailscale enabled successfully");
        debug!("tailscale up output: {}", stdout.trim());
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);
        warn!("✗ Tailscale up failed (exit code {}): {}", exit_code, stderr);
        Err(TailscaleError::CommandFailed(stderr.to_string()))
    }
}

/// Execute `tailscale down` to disable Tailscale
pub fn tailscale_down() -> TailscaleResult<()> {
    info!("→ Disabling Tailscale (executing: tailscale down)");

    if !is_tailscale_installed() {
        warn!("Tailscale executable not found in PATH");
        return Err(TailscaleError::NotFound("tailscale executable not found".to_string()));
    }

    let output = Command::new("tailscale")
        .arg("down")
        .output()
        .map_err(|e| {
            warn!("Failed to execute tailscale down command: {}", e);
            TailscaleError::CommandFailed(format!("Failed to execute tailscale down: {}", e))
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("✓ Tailscale disabled successfully");
        debug!("tailscale down output: {}", stdout.trim());
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);
        warn!("✗ Tailscale down failed (exit code {}): {}", exit_code, stderr);
        Err(TailscaleError::CommandFailed(stderr.to_string()))
    }
}

// ///Check if Tailscale is currently running (connected)
// pub fn is_tailscale_running() -> bool {
//     debug!("Checking if Tailscale is currently running");

//     if !is_tailscale_installed() {
//         debug!("Tailscale not installed, cannot check running status");
//         return false;
//     }

//     let output = Command::new("tailscale")
//         .args(["status", "-json"])
//         .output();

//     match output {
//         Ok(out) => {
//             if out.status.success() {
//                 if let Ok(text) = std::str::from_utf8(&out.stdout) {
//                     if let Ok(json) = json::parse(text) {
//                         let running = json["BackendState"]
//                             .as_str()
//                             .map(|s| s == "Running")
//                             .unwrap_or(false);
//                         debug!("Tailscale running status: {}", running);
//                         return running;
//                     }
//                 }
//             }
//             debug!("Tailscale status command returned non-success");
//             false
//         }
//         Err(e) => {
//             debug!("Failed to check Tailscale status: {}", e);
//             false
//         }
//     }
// }
