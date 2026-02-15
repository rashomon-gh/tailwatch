use std::fs;
use std::path::Path;

const SERVICE_TEMPLATE: &str = r#"[Unit]
Description=Network-aware Tailscale daemon
Documentation=https://codeberg.org/rashomon/tailwatch
After=network-online.target
Wants=network-online.target
ConditionPathExists=/usr/local/bin/tailwatch

[Service]
Type=simple
# Change --blocked-ssid to the WiFi SSID(s) that should disable Tailscale
ExecStart=/usr/local/bin/tailwatch run {{SSID_ARGS}}
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/run
RestrictRealtime=true
RestrictAddressFamilies=AF_UNIX

# D-Bus access for NetworkManager
AllowBus=org.freedesktop.NetworkManager

# Logging
StandardError=journal
StandardOutput=journal

[Install]
WantedBy=multi-user.target
"#;

/// Generates a systemd service file with the specified WiFi SSIDs
///
/// # Arguments
/// * `ssids` - The WiFi SSIDs that should be inserted into the service file
/// * `output_path` - The path where the generated service file should be written
///
/// # Errors
/// Returns an error if the file cannot be written
pub fn generate_service_file(ssids: &[String], output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Build the command line arguments for all SSIDs
    let ssid_args: String = ssids
        .iter()
        .map(|ssid| format!("--blocked-ssid {}", ssid))
        .collect::<Vec<_>>()
        .join(" ");

    // Replace the placeholder SSID arguments in the template
    let service_content = SERVICE_TEMPLATE.replace("{{SSID_ARGS}}", &ssid_args);

    // Write the generated service file
    fs::write(output_path, service_content)?;

    Ok(())
}
