# WAT 
When you have an ESP but want Router
Station(Sta) + Access Point(Ap) in mixed mode with NAT 

## Binaries
This project provides two binaries:
1. **esp-wifi-ap**: Wi-Fi Access Point with client distance measurement using RTT
2. **esp-wifi-client**: Wi-Fi Station client with RSSI-based distance estimation

## Features
- **Device Naming**: Friendly device names generated from MAC addresses
- **Distance Measurement**: 
  - AP: RTT (Round Trip Time) for precise ranging
  - Client: RSSI-based distance estimation
- **Chip Support**: ESP32-C6 and ESP32-C3
- **Robust Logging**: Comprehensive Wi-Fi event and connection status logging

# How
```bash
cp .env.example .env
```

install ESP_IDF (google it!)
run `export.bash` or `export.fish` from ESP_IDF
get tag 5.2.2
It has to say sth like:
```bash
idf.py --version
ESP-IDF v5.2.3-dirty
```

## run

### Wi-Fi Access Point
```bash
cargo build --bin esp-wifi-ap
cargo espflash flash --release --bin esp-wifi-ap
# OR using tasks
cargo run --bin esp-wifi-ap
```

### Wi-Fi Station Client  
```bash
cargo build --bin esp-wifi-client
cargo espflash flash --release --bin esp-wifi-client
# OR using tasks  
cargo run --bin esp-wifi-client
```

### Using justfile
```bash
cargo install
just run  # Runs the AP by default
```

## Environment Variables
Make sure to set up your `.env` file:
```bash
cp .env.example .env
# Edit .env with your Wi-Fi credentials:
# ST_SSID=YourWiFiNetwork
# ST_PASS=YourWiFiPassword  
# AP_SSID=rust-was-here
# AP_PASS=change-me-for-your-own
```

## RSSI Distance Estimation
The client uses RSSI (Received Signal Strength Indicator) to estimate distance to the AP:
- **Formula**: `Distance = 10^((RSSI_ref - RSSI) / (10 * n))`
- **RSSI_ref**: -30 dBm (reference at 1 meter)
- **Path Loss Exponent (n)**: 2.0 (free space)
- **Distance Ranges**:
  - Very Close: <1m
  - Close: 1-5m  
  - Medium: 5-15m
  - Far: 15-50m
  - Very Far: >50m

**Note**: RSSI-based distance is an approximation and can vary significantly based on environment, obstacles, and interference.
