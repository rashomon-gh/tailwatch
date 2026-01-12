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
    blocked_ssid: String,
}

const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

impl DaemonState {
    pub fn new(blocked_ssid: String) -> Self {
        info!("Initializing daemon state with blocked SSID: \"{}\"", blocked_ssid);
        Self {
            current_state: Arc::new(Mutex::new(StateInner {
                network: NetworkState::default(),
                tailscale_enabled: true,
                last_change_time: None,
                pending_state: None,
                blocked_ssid,
            })),
        }
    }

    /// Update the network state
    pub async fn update_network(&self, network: NetworkState) {
        let mut state = self.current_state.lock().await;
        debug!(
            "Network state updated: ethernet={}, wifi_ssid={:?}",
            network.ethernet_active, network.wifi_ssid
        );
        state.network = network;
    }

    /// Evaluate whether Tailscale should be disabled based on current network state
    fn should_disable_tailscale(network: &NetworkState, blocked_ssid: &str) -> bool {
        let should_disable = network.ethernet_active
            || network.wifi_ssid.as_deref() == Some(blocked_ssid);

        if should_disable {
            debug!(
                "Tailscale should be DISABLED: ethernet active={}, blocked wifi={}, blocked_ssid={:?}",
                network.ethernet_active,
                network.wifi_ssid.as_deref() == Some(blocked_ssid),
                network.wifi_ssid
            );
        } else {
            debug!("Tailscale should be ENABLED: no blocking conditions");
        }

        should_disable
    }

    /// Check the current state and update Tailscale if needed
    pub async fn evaluate_and_update(&self) -> Result<(), TailscaleError> {
        let mut state = self.current_state.lock().await;
        let blocked_ssid = state.blocked_ssid.clone();
        let should_disable = Self::should_disable_tailscale(&state.network, &blocked_ssid);

        debug!(
            "Evaluating: ethernet={}, wifi_ssid={:?}, should_disable={}, current_enabled={}",
            state.network.ethernet_active, state.network.wifi_ssid, should_disable, state.tailscale_enabled
        );

        // If the desired state matches the current state, nothing to do
        if should_disable == !state.tailscale_enabled {
            debug!(
                "No change needed: Tailscale is already {}",
                if state.tailscale_enabled { "enabled" } else { "disabled" }
            );
            return Ok(());
        }

        let desired_state = !should_disable;
        info!(
            "State change requested: {} → {}",
            if state.tailscale_enabled { "enabled" } else { "disabled" },
            if desired_state { "enabled" } else { "disabled" }
        );

        // Set pending state and record change time
        state.pending_state = Some(desired_state);
        state.last_change_time = Some(Instant::now());

        info!("Starting {}s debounce timer for state transition", DEBOUNCE_DURATION.as_secs());
        drop(state);

        // Spawn debounce task
        let state_clone = self.current_state.clone();
        tokio::spawn(async move {
            sleep(DEBOUNCE_DURATION).await;
            debug!("Debounce timer elapsed, applying pending state");
            apply_pending_state_impl(state_clone).await;
        });

        Ok(())
    }

    /// Initialize the state by checking the current network and setting Tailscale appropriately
    pub async fn initialize(&self, network: NetworkState) -> Result<(), TailscaleError> {
        let mut state = self.current_state.lock().await;
        state.network = network.clone();
        let blocked_ssid = state.blocked_ssid.clone();

        let should_disable = Self::should_disable_tailscale(&network, &blocked_ssid);

        info!("═══════════════════════════════════════");
        info!("Initial network assessment:");
        info!("  Ethernet active: {}", network.ethernet_active);
        info!("  WiFi SSID: {:?}", network.wifi_ssid);
        info!("  Blocked SSID configured: \"{}\"", blocked_ssid);
        info!("  Blocked SSID detected: {}",
            network.wifi_ssid.as_deref() == Some(&blocked_ssid));
        info!("  Should disable Tailscale: {}", should_disable);
        info!("═══════════════════════════════════════");

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
        Self::new("tue-wpa2".to_string())
    }
}

/// Apply the pending state after debounce (boxed to avoid recursion size issues)
async fn apply_pending_state_impl(state: Arc<Mutex<StateInner>>) {
    let mut inner = state.lock().await;

    // Check if we still have a pending state
    let Some(desired_enabled) = inner.pending_state else {
        debug!("No pending state to apply (may have been cancelled)");
        return;
    };

    let blocked_ssid = inner.blocked_ssid.clone();
    debug!("Applying pending state: enable={}, blocked_ssid={}", desired_enabled, blocked_ssid);

    // Clear the pending state
    inner.pending_state = None;

    // Check if the desired state still matches what we want
    let should_disable = DaemonState::should_disable_tailscale(&inner.network, &blocked_ssid);
    let current_desired = !should_disable;

    if current_desired != desired_enabled {
        // State changed during debounce, restart debounce
        info!(
            "⚠ Network state changed during debounce (wanted: enable={}, now should: enable={}), restarting timer",
            desired_enabled, current_desired
        );
        inner.pending_state = Some(current_desired);
        inner.last_change_time = Some(Instant::now());
        let state_clone = state.clone();
        drop(inner);

        sleep(DEBOUNCE_DURATION).await;
        debug!("Restarted debounce timer completed, re-evaluating");
        Box::pin(apply_pending_state_impl(state_clone)).await;
        return;
    }

    info!("✓ Debounce passed, applying state change: enable={}", desired_enabled);

    // Apply the state change
    if desired_enabled {
        match tailscale_up() {
            Ok(()) => {
                inner.tailscale_enabled = true;
                info!("✓ Tailscale state transition complete: ENABLED");
            }
            Err(e) => {
                error!("Failed to enable Tailscale: {}", e);
                inner.tailscale_enabled = false; // Assume still disabled
            }
        }
    } else {
        match tailscale_down() {
            Ok(()) => {
                inner.tailscale_enabled = false;
                info!("✓ Tailscale state transition complete: DISABLED");
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
