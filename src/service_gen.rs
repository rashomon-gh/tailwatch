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
# Change --blocked-ssid to the WiFi SSID that should disable Tailscale
ExecStart=/usr/local/bin/tailwatch run --blocked-ssid {{SSID}}
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

/// Generates a systemd service file with the specified WiFi SSID
///
/// # Arguments
/// * `ssid` - The WiFi SSID that should be inserted into the service file
/// * `output_path` - The path where the generated service file should be written
///
/// # Errors
/// Returns an error if the file cannot be written
pub fn generate_service_file(ssid: &str, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Replace the placeholder SSID in the template
    let service_content = SERVICE_TEMPLATE.replace("{{SSID}}", ssid);

    // Write the generated service file
    fs::write(output_path, service_content)?;

    Ok(())
}

/// Returns the service template as a string with the SSID placeholder replaced
///
/// # Arguments
/// * `ssid` - The WiFi SSID to insert into the template
///
/// # Returns
/// The service file content with the specified SSID
pub fn get_service_template(ssid: &str) -> String {
    SERVICE_TEMPLATE.replace("{{SSID}}", ssid)
}
