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
- **Network Cycling**: Client can cycle through multiple Wi-Fi networks with button press
- **Auto-reconnection**: Automatic reconnection handling when networks become unavailable

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

# Access Point settings
AP_SSID=rust-was-here
AP_PASS=change-me-for-your-own

# Multiple Wi-Fi networks for client cycling
ST_SSID_1=HomeWifi
ST_PASS_1=homepassword123

ST_SSID_2=WorkWifi
ST_PASS_2=workpassword456

ST_SSID_3=GuestWifi
ST_PASS_3=guestpassword789
```

## Network Cycling (Client Mode)
The client supports cycling through multiple Wi-Fi networks:

1. **Configure multiple networks** in your `.env` file using the format `ST_SSID_X` and `ST_PASS_X`
2. **Press GPIO0 button** (boot button) to cycle to the next network
3. **Automatic wrap-around**: After the last network, it cycles back to the first
4. **Real-time feedback**: Shows which network is currently selected and connection status
5. **Auto-reconnection**: Attempts to reconnect if connection is lost

### Button Controls
- **GPIO0 (Boot Button)**: Cycle to next Wi-Fi network
- **Hold button**: Immediate network switching (disconnects current, connects to next)

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
