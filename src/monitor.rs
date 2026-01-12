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
}

impl NetworkMonitor {
    /// Create a new network monitor
    pub async fn new(state: DaemonState) -> Result<Self, Box<dyn std::error::Error>> {
        let connection = Connection::system().await?;

        Ok(Self {
            connection,
            state,
            last_state: NetworkState::default(),
        })
    }

    /// Get the current network state
    pub async fn get_network_state(&self) -> NetworkState {
        let mut network_state = NetworkState::default();

        // Get all devices from NetworkManager
        let devices = match self.get_devices().await {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to get devices: {}", e);
                return network_state;
            }
        };

        debug!("Found {} devices", devices.len());

        // Check each device
        for device_path in devices {
            if let Ok(device) = self.get_device_proxy(&device_path).await {
                if let Ok(device_type) = self.get_device_type(&device).await {
                    // Only consider active devices
                    if self.is_device_active(&device).await.unwrap_or(false) {
                        match device_type {
                            NM_DEVICE_TYPE_ETHERNET => {
                                info!("Active ethernet device found: {}", device_path);
                                network_state.ethernet_active = true;
                            }
                            NM_DEVICE_TYPE_WIFI => {
                                if let Some(ssid) = self.get_wifi_ssid(&device_path).await {
                                    info!("Active WiFi device: {}, SSID: {:?}", device_path, ssid);
                                    network_state.wifi_ssid = Some(ssid);
                                }
                            }
                            _ => {
                                debug!("Ignoring device type {} on {}", device_type, device_path);
                            }
                        }
                    }
                }
            }
        }

        network_state
    }

    /// Get all device paths from NetworkManager
    async fn get_devices(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_OBJECT_PATH,
            NM_INTERFACE,
        )
        .await?;

        let devices = proxy
            .call::<&str, (), Vec<String>>("GetDevices", &())
            .await?;
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
                debug!("No active access point: {}", e);
                return None;
            }
        };

        // Skip if it's the "/" (no access point)
        if ap_path == "/" {
            return None;
        }

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
        String::from_utf8(ssid_bytes).ok()
    }

    /// Monitor for network changes and update state accordingly
    pub async fn monitor_network_changes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting network monitoring");

        loop {
            // Get the new network state
            let network_state = self.get_network_state().await;

            // Check if state changed
            if network_state != self.last_state {
                info!("Network state changed from {:?} to {:?}", self.last_state, network_state);
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
