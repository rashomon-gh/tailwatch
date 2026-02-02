mod monitor;
mod service_gen;
mod state;
mod tailscale;

use clap::Parser;
use monitor::NetworkMonitor;
use state::DaemonState;
use std::path::Path;
use tracing::{error, info};
use tracing_journald::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Network-aware Tailscale management daemon
#[derive(Parser, Debug)]
#[command(name = "tailwatch")]
#[command(version = VERSION)]
#[command(about = "Network-aware Tailscale management daemon", long_about = None)]
enum Command {
    /// Run the tailwatch daemon
    Run {
        /// WiFi SSID that should trigger Tailscale to be disabled
        #[arg(short, long)]
        blocked_ssid: String,
    },
    /// Generate a systemd service file
    GenerateService {
        /// WiFi SSID to use in the service file
        #[arg(short, long)]
        ssid: String,
        /// Output path for the generated service file (default: tailwatch.service)
        #[arg(short, long, default_value = "tailwatch.service")]
        output: String,
    },
}

#[tokio::main]
async fn main() {
    // Parse command-line arguments first
    let command = Command::parse();

    match command {
        Command::Run { blocked_ssid } => {
            // Initialize logging to systemd journal
            let layer = Layer::new().unwrap();

            tracing_subscriber::registry()
                .with(layer)
                .with(tracing_subscriber::filter::LevelFilter::INFO)
                .init();

            info!("═══════════════════════════════════════");
            info!("Tailwatch v{} starting", VERSION);
            info!("Network-aware Tailscale management daemon");
            info!("Blocked SSID: \"{}\"", blocked_ssid);
            info!("═══════════════════════════════════════");

            // Check if Tailscale is installed
            info!("Checking Tailscale installation...");
            if !tailscale::is_tailscale_installed() {
                error!("Tailscale is not installed or not in PATH");
                error!("Please install Tailscale first: https://tailscale.com/download/");
                std::process::exit(1);
            }
            info!("✓ Tailscale found in PATH");

            // Create daemon state with the blocked SSID
            let state = DaemonState::new(blocked_ssid);

            // Create network monitor
            info!("Initializing network monitor...");
            let mut monitor = match NetworkMonitor::new(state.clone()).await {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to create network monitor: {}", e);
                    error!("Make sure NetworkManager is running");
                    std::process::exit(1);
                }
            };

            // Get initial network state
            info!("Performing initial network scan...");
            let initial_network_state = monitor.get_network_state().await;

            // Initialize Tailscale state
            info!("Initializing Tailscale state based on current network...");
            if let Err(e) = state.initialize(initial_network_state).await {
                error!("Failed to initialize state: {}", e);
            }

            info!("✓ Initialization complete, entering monitoring loop");
            info!("Log output: journalctl -u tailwatch -f");
            info!("═══════════════════════════════════════");

            // Start monitoring for network changes
            if let Err(e) = monitor.monitor_network_changes().await {
                error!("Network monitoring failed: {}", e);
                std::process::exit(1);
            }
        }
        Command::GenerateService { ssid, output } => {
            let output_path = Path::new(&output);

            match service_gen::generate_service_file(&ssid, output_path) {
                Ok(()) => {
                    println!("✓ Service file generated successfully: {}", output);
                    println!("  Blocked SSID: \"{}\"", ssid);
                    println!("\nTo install the service:");
                    println!("  sudo cp {} /etc/systemd/system/", output);
                    println!("  sudo systemctl daemon-reload");
                    println!("  sudo systemctl enable tailwatch");
                    println!("  sudo systemctl start tailwatch");
                }
                Err(e) => {
                    eprintln!("Error generating service file: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
