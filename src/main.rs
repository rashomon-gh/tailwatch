mod monitor;
mod state;
mod tailscale;

use monitor::NetworkMonitor;
use state::DaemonState;
use tracing::{error, info};
use tracing_journald::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize logging to systemd journal
    let layer = Layer::new().unwrap();

    tracing_subscriber::registry()
        .with(layer)
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .init();

    info!("Starting Tailwatch daemon");

    // Check if Tailscale is installed
    if !tailscale::is_tailscale_installed() {
        error!("Tailscale is not installed or not in PATH");
        std::process::exit(1);
    }

    // Create daemon state
    let state = DaemonState::new();

    // Create network monitor
    let mut monitor = match NetworkMonitor::new(state.clone()).await {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create network monitor: {}", e);
            std::process::exit(1);
        }
    };

    // Get initial network state
    info!("Getting initial network state");
    let initial_network_state = monitor.get_network_state().await;

    // Initialize Tailscale state
    info!("Initializing Tailscale state");
    if let Err(e) = state.initialize(initial_network_state).await {
        error!("Failed to initialize state: {}", e);
    }

    // Start monitoring for network changes
    info!("Starting network monitoring loop");

    if let Err(e) = monitor.monitor_network_changes().await {
        error!("Network monitoring failed: {}", e);
        std::process::exit(1);
    }
}
