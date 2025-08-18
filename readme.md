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
- **Chip Support**: ESP32-C6 (default) and ESP32-C3
- **Robust Logging**: Comprehensive Wi-Fi event and connection status logging
- **Network Cycling**: Client can cycle through multiple Wi-Fi networks with button press
- **Auto-reconnection**: Automatic reconnection handling when networks become unavailable

## Hardware Support

### ESP32-C6 (Default)
- **Target**: `riscv32imac-esp-espidf`
- **Chip**: esp32c6
- **Default configuration** - no additional flags needed

### ESP32-C3 (Optional Feature)
- **Target**: `riscv32imc-esp-espidf` 
- **Chip**: esp32c3
- **Feature flag**: `--features esp32c3`
- **Architecture**: RISC-V 32-bit single-core @ 160 MHz
- **Memory**: 400 KB SRAM, 384 KB ROM

### Key Differences
| Feature | ESP32-C6 | ESP32-C3 |
|---------|----------|----------|
| Architecture | RISC-V 32-bit dual-core | RISC-V 32-bit single-core |
| CPU Speed | 160 MHz | 160 MHz |
| Target | `riscv32imac-esp-espidf` | `riscv32imc-esp-espidf` |
| Wi-Fi | 802.11 b/g/n | 802.11 b/g/n |
| Bluetooth | LE 5.0 + Zigbee/Thread | LE 5.0 |
| Build Command | `just build` | `just build-c3` |

# Setup
```bash
cp .env.example .env
```

Install ESP_IDF (google it!)
run `export.bash` or `export.fish` from ESP_IDF
get tag  v5.4.1
It has to say sth like:
```bash
idf.py --version
ESP-IDF v5.4.1
```

## Build & Flash

### Wi-Fi Access Point (C6)
```bash
# Using justfile (recommended)
just build        # Build for ESP32-C6
just flash        # Flash to ESP32-C6
just run          # Build, flash, and monitor

# Or using cargo directly
cargo build --release --target riscv32imac-esp-espidf
espflash flash --monitor --chip esp32c6 target/riscv32imac-esp-espidf/release/esp-wifi-ap
```

### Wi-Fi Access Point (C3)
```bash
# Using justfile (recommended)
just build-c3     # Build for ESP32-C3
just flash-c3     # Flash to ESP32-C3  
just run-c3       # Build, flash, and monitor ESP32-C3

# Or using cargo directly
MCU=esp32c3 cargo build --release --target riscv32imc-esp-espidf --features esp32c3
espflash flash --monitor --chip esp32c3 target/riscv32imc-esp-espidf/release/esp-wifi-ap
```

### Wi-Fi Station Client  
```bash
# ESP32-C6 (default)
cargo build --bin esp-wifi-client --release
cargo espflash flash --release --bin esp-wifi-client

# ESP32-C3 
MCU=esp32c3 cargo build --bin esp-wifi-client --release --target riscv32imc-esp-espidf --features esp32c3
espflash flash --monitor --chip esp32c3 target/riscv32imc-esp-espidf/release/esp-wifi-client
# OR using tasks  
cargo run --bin esp-wifi-client
```

### Available Just Commands
```bash
# ESP32-C6 (Default)
just build          # Build for ESP32-C6
just flash          # Flash ESP32-C6
just run            # Build, flash, and monitor ESP32-C6 (AP mode)
just run-client     # Build, flash, and monitor ESP32-C6 (Client mode)

# ESP32-C3 (Feature)  
just build-c3       # Build for ESP32-C3
just flash-c3       # Flash ESP32-C3
just run-c3         # Build, flash, and monitor ESP32-C3 (AP mode)
just run-client-c3  # Build, flash, and monitor ESP32-C3 (Client mode)

# Utility commands
just where_my_esp_at    # Find ESP device ports
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
