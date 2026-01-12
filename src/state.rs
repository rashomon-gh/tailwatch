use crate::tailscale::{tailscale_down, tailscale_up, TailscaleError};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};
use tracing::{debug, error, info, warn};

/// Network state information
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetworkState {
    pub ethernet_active: bool,
    pub wifi_ssid: Option<String>,
}

/// Daemon state for managing Tailscale
#[derive(Clone)]
pub struct DaemonState {
    current_state: Arc<Mutex<StateInner>>,
}

#[derive(Debug)]
struct StateInner {
    network: NetworkState,
    tailscale_enabled: bool,
    last_change_time: Option<Instant>,
    pending_state: Option<bool>,
}

const BLOCKED_WIFI_SSID: &str = "tue-wpa2";
const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

impl DaemonState {
    pub fn new() -> Self {
        Self {
            current_state: Arc::new(Mutex::new(StateInner {
                network: NetworkState::default(),
                tailscale_enabled: true,
                last_change_time: None,
                pending_state: None,
            })),
        }
    }

    /// Update the network state
    pub async fn update_network(&self, network: NetworkState) {
        let mut state = self.current_state.lock().await;
        debug!("Network state updated: {:?}", network);
        state.network = network;
    }

    /// Evaluate whether Tailscale should be disabled based on current network state
    pub fn should_disable_tailscale(network: &NetworkState) -> bool {
        // Disable if ethernet is active OR connected to blocked WiFi SSID
        network.ethernet_active || network.wifi_ssid.as_deref() == Some(BLOCKED_WIFI_SSID)
    }

    /// Check the current state and update Tailscale if needed
    pub async fn evaluate_and_update(&self) -> Result<(), TailscaleError> {
        let mut state = self.current_state.lock().await;
        let should_disable = Self::should_disable_tailscale(&state.network);

        debug!(
            "Evaluating: ethernet={}, wifi_ssid={:?}, should_disable={}",
            state.network.ethernet_active, state.network.wifi_ssid, should_disable
        );

        // If the desired state matches the current state, nothing to do
        if should_disable == !state.tailscale_enabled {
            debug!("Tailscale state already correct (enabled={})", state.tailscale_enabled);
            return Ok(());
        }

        // Set pending state and record change time
        state.pending_state = Some(!should_disable);
        state.last_change_time = Some(Instant::now());

        drop(state);

        // Spawn debounce task
        let state_clone = self.current_state.clone();
        tokio::spawn(async move {
            sleep(DEBOUNCE_DURATION).await;
            apply_pending_state_impl(state_clone).await;
        });

        Ok(())
    }

    /// Initialize the state by checking the current network and setting Tailscale appropriately
    pub async fn initialize(&self, network: NetworkState) -> Result<(), TailscaleError> {
        let mut state = self.current_state.lock().await;
        state.network = network.clone();

        let should_disable = Self::should_disable_tailscale(&network);

        info!(
            "Initial state: ethernet={}, wifi_ssid={:?}, should_disable={}",
            state.network.ethernet_active, state.network.wifi_ssid, should_disable
        );

        if should_disable {
            info!("Initial state: Disabling Tailscale");
            if let Err(e) = tailscale_down() {
                error!("Failed to disable Tailscale on startup: {}", e);
            }
            state.tailscale_enabled = false;
        } else {
            info!("Initial state: Enabling Tailscale");
            if let Err(e) = tailscale_up() {
                error!("Failed to enable Tailscale on startup: {}", e);
            }
            state.tailscale_enabled = true;
        }

        Ok(())
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply the pending state after debounce (boxed to avoid recursion size issues)
async fn apply_pending_state_impl(state: Arc<Mutex<StateInner>>) {
    let mut inner = state.lock().await;

    // Check if we still have a pending state
    let Some(desired_enabled) = inner.pending_state else {
        return;
    };

    // Clear the pending state
    inner.pending_state = None;

    // Check if the desired state still matches what we want
    let should_disable = DaemonState::should_disable_tailscale(&inner.network);
    let current_desired = !should_disable;

    if current_desired != desired_enabled {
        // State changed during debounce, restart debounce
        info!("Network state changed during debounce, restarting debounce timer");
        inner.pending_state = Some(current_desired);
        inner.last_change_time = Some(Instant::now());
        let state_clone = state.clone();
        drop(inner);

        sleep(DEBOUNCE_DURATION).await;
        Box::pin(apply_pending_state_impl(state_clone)).await;
        return;
    }

    // Apply the state change
    if desired_enabled {
        info!("Enabling Tailscale");
        match tailscale_up() {
            Ok(()) => {
                inner.tailscale_enabled = true;
                info!("Tailscale enabled successfully");
            }
            Err(e) => {
                error!("Failed to enable Tailscale: {}", e);
            }
        }
    } else {
        info!("Disabling Tailscale");
        match tailscale_down() {
            Ok(()) => {
                inner.tailscale_enabled = false;
                info!("Tailscale disabled successfully");
            }
            Err(e) => {
                warn!("Failed to disable Tailscale: {}", e);
                // Continue with the state change even if tailscale down failed
                // (e.g., if tailscale was already down)
                inner.tailscale_enabled = false;
            }
        }
    }
}
