use crate::state::{DaemonState, NetworkState};
use tracing::{debug, error, info, warn};
use zbus::{Connection, Proxy};

/// NetworkManager D-Bus interface constants
const NM_BUS_NAME: &str = "org.freedesktop.NetworkManager";
const NM_OBJECT_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_INTERFACE: &str = "org.freedesktop.NetworkManager";

const NM_DEVICE_INTERFACE: &str = "org.freedesktop.NetworkManager.Device";
const NM_DEVICE_TYPE_ETHERNET: u32 = 1;
const NM_DEVICE_TYPE_WIFI: u32 = 2;

const NM_AP_INTERFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
const NM_WIFI_INTERFACE: &str = "org.freedesktop.NetworkManager.Device.Wireless";

/// Network monitor using NetworkManager via D-Bus
pub struct NetworkMonitor {
    connection: Connection,
    state: DaemonState,
    last_state: NetworkState,
    poll_count: u64,
}

impl NetworkMonitor {
    /// Create a new network monitor
    pub async fn new(state: DaemonState) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Connecting to NetworkManager via D-Bus");
        let connection = Connection::system().await?;
        info!("Successfully connected to system D-Bus");

        Ok(Self {
            connection,
            state,
            last_state: NetworkState::default(),
            poll_count: 0,
        })
    }

    /// Get the current network state
    pub async fn get_network_state(&self) -> NetworkState {
        let mut network_state = NetworkState::default();
        let mut active_devices = 0;

        // Get all devices from NetworkManager
        let devices = match self.get_devices().await {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to get devices from NetworkManager: {}", e);
                return network_state;
            }
        };

        debug!("Found {} network devices", devices.len());
        let total_devices = devices.len();

        // Check each device
        for device_path in devices {
            if let Ok(device) = self.get_device_proxy(&device_path).await {
                if let Ok(device_type) = self.get_device_type(&device).await {
                    // Only consider active devices
                    match self.is_device_active(&device).await {
                        Ok(true) => {
                            active_devices += 1;
                            match device_type {
                                NM_DEVICE_TYPE_ETHERNET => {
                                    network_state.ethernet_active = true;
                                }
                                NM_DEVICE_TYPE_WIFI => {
                                    if let Some(ssid) = self.get_wifi_ssid(&device_path).await {
                                        network_state.wifi_ssid = Some(ssid);
                                    }
                                }
                                _ => {
                                    debug!(
                                        "Ignoring device type {} on {}",
                                        device_type, device_path
                                    );
                                }
                            }
                        }
                        Ok(false) => {
                            debug!(
                                "Device {} not active (disconnected/Unavailable)",
                                device_path
                            );
                        }
                        Err(e) => {
                            warn!("Failed to check active state for {}: {}", device_path, e);
                        }
                    }
                }
            }
        }

        debug!(
            "Network scan complete: {}/{} devices active",
            active_devices, total_devices
        );

        network_state
    }

    /// Get all device paths from NetworkManager
    async fn get_devices(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        use zbus::zvariant::OwnedObjectPath;

        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_OBJECT_PATH,
            NM_INTERFACE,
        )
        .await?;

        let device_paths: Vec<OwnedObjectPath> = proxy
            .call::<&str, (), Vec<OwnedObjectPath>>("GetDevices", &())
            .await?;

        // Convert OwnedObjectPath to String
        let devices = device_paths
            .into_iter()
            .map(|p| p.as_str().to_string())
            .collect();

        Ok(devices)
    }

    /// Get a device proxy
    async fn get_device_proxy<'a>(
        &'a self,
        device_path: &'a str,
    ) -> Result<Proxy<'a>, Box<dyn std::error::Error>> {
        Ok(Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            NM_DEVICE_INTERFACE,
        )
        .await?)
    }

    /// Get the device type
    async fn get_device_type(&self, device: &Proxy<'_>) -> Result<u32, Box<dyn std::error::Error>> {
        let device_type: u32 = device.get_property::<u32>("DeviceType").await?;
        Ok(device_type)
    }

    /// Check if a device is active (connected)
    async fn is_device_active(&self, device: &Proxy<'_>) -> Result<bool, Box<dyn std::error::Error>> {
        let state: u32 = device.get_property::<u32>("State").await?;
        // NM_DEVICE_STATE_ACTIVATED = 100
        Ok(state == 100)
    }

    /// Get the SSID of the currently connected WiFi network
    async fn get_wifi_ssid(&self, device_path: &str) -> Option<String> {
        // Get the wireless interface for the device
        let wifi_proxy = match Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            NM_WIFI_INTERFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to get wireless interface for {}: {}", device_path, e);
                return None;
            }
        };

        // Get the active access point path
        let ap_path: String = match wifi_proxy.get_property("ActiveAccessPoint").await {
            Ok(p) => p,
            Err(e) => {
                debug!("No active access point for {}: {}", device_path, e);
                return None;
            }
        };

        // Skip if it's the "/" (no access point)
        if ap_path == "/" {
            debug!("WiFi device {} has no active access point", device_path);
            return None;
        }

        debug!("WiFi device {} connected to access point: {}", device_path, ap_path);

        // Get the SSID from the access point
        let ap_proxy = match Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            ap_path.as_str(),
            NM_AP_INTERFACE,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to get access point proxy for {}: {}", ap_path, e);
                return None;
            }
        };

        // SSID is returned as a byte array (ay in D-Bus)
        let ssid_bytes: Vec<u8> = match ap_proxy.get_property("Ssid").await {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to get SSID for {}: {}", ap_path, e);
                return None;
            }
        };

        // Convert bytes to String (SSID is UTF-8)
        match String::from_utf8(ssid_bytes) {
            Ok(ssid) => {
                debug!("Retrieved SSID: {:?}", ssid);
                Some(ssid)
            }
            Err(e) => {
                warn!("Failed to parse SSID as UTF-8: {}", e);
                None
            }
        }
    }

    /// Monitor for network changes and update state accordingly
    pub async fn monitor_network_changes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let blocked_ssid = self.state.get_blocked_ssid().await;

        info!("═══════════════════════════════════════");
        info!("Starting network monitoring loop");
        info!("Polling interval: 2 seconds");
        info!("Blocked SSID: \"{}\"", blocked_ssid);
        info!("═══════════════════════════════════════");

        loop {
            self.poll_count += 1;

            // Log heartbeat every 60 polls (2 minutes)
            if self.poll_count % 60 == 0 {
                info!(
                    "💓 Heartbeat: Uptime: {} polls, Current state: ethernet={}, wifi={:?}",
                    self.poll_count,
                    self.last_state.ethernet_active,
                    self.last_state.wifi_ssid
                );
            }

            // Get the new network state
            let network_state = self.get_network_state().await;

            // Check if state changed
            if network_state != self.last_state {
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                info!("⚠ Network state DETECTED!");
                info!("  Previous: ethernet={}, wifi={:?}",
                    self.last_state.ethernet_active, self.last_state.wifi_ssid);
                info!("  Current:  ethernet={}, wifi={:?}",
                    network_state.ethernet_active, network_state.wifi_ssid);
                info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                self.last_state = network_state.clone();
                self.state.update_network(network_state).await;

                // Evaluate and update Tailscale if needed
                if let Err(e) = self.state.evaluate_and_update().await {
                    error!("Failed to update Tailscale state: {}", e);
                }
            }

            // Poll every 2 seconds
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}
