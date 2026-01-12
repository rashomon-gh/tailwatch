mod monitor;
mod state;
mod tailscale;

use clap::Parser;
use monitor::NetworkMonitor;
use state::DaemonState;
use tracing::{error, info};
use tracing_journald::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Network-aware Tailscale management daemon
#[derive(Parser, Debug)]
#[command(name = "tailwatch")]
#[command(version = VERSION)]
#[command(about = "Network-aware Tailscale management daemon", long_about = None)]
struct Args {
    /// WiFi SSID that should trigger Tailscale to be disabled
    #[arg(short, long)]
    blocked_ssid: String,
}

#[tokio::main]
async fn main() {
    // Parse command-line arguments first
    let args = Args::parse();

    // Initialize logging to systemd journal
    let layer = Layer::new().unwrap();

    tracing_subscriber::registry()
        .with(layer)
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .init();

    info!("═══════════════════════════════════════");
    info!("Tailwatch v{} starting", VERSION);
    info!("Network-aware Tailscale management daemon");
    info!("Blocked SSID: \"{}\"", args.blocked_ssid);
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
    let state = DaemonState::new(args.blocked_ssid);

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
