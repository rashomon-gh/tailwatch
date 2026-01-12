# Tailwatch

Tailwatch is a network-aware daemon that automatically disables Tailscale when connected to specific networks—either when connected to WiFi SSID "tue-wpa2" or when using an ethernet connection. When these conditions no longer apply, it automatically re-enables Tailscale. The daemon polls NetworkManager every 2 seconds and includes a debounce delay to prevent rapid toggling.

**Tested on Fedora 43 only.**

## Running

Install as a systemd service:

```bash
cargo build --release
sudo cp target/release/tailwatch /usr/local/bin/
sudo cp tailwatch.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tailwatch
```

View logs with `journalctl -u tailwatch -f`.
