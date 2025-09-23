# MAC Address to Hostname Configuration

This document explains how to configure static MAC address to hostname mappings for your ESP32 router's DNS server.

## Overview

By default, the router assigns dynamic hostnames to connected devices using a pool of friendly names (e.g., `ancient-waterfall.local`). However, you can assign specific, permanent hostnames to devices based on their MAC addresses.

## Configuration Methods

### Method 1: Environment Variable (Recommended)

Add MAC hostname mappings to your `.env` file using the `MAC_HOSTNAMES` variable:

```bash
# Format: MAC_HOSTNAMES=mac1:hostname1,mac2:hostname2,...
MAC_HOSTNAMES=aa:bb:cc:dd:ee:ff:my-laptop,11:22:33:44:55:66:raspberry-pi,77:88:99:aa:bb:cc:arduino-iot
```

### Method 2: Individual Environment Variables

You can also define individual mappings:

```bash
MAC_HOSTNAME_1=aa:bb:cc:dd:ee:ff:my-laptop
MAC_HOSTNAME_2=11:22:33:44:55:66:raspberry-pi
MAC_HOSTNAME_3=77:88:99:aa:bb:cc:arduino-iot
MAC_HOSTNAME_4=dd:ee:ff:11:22:33:security-camera
MAC_HOSTNAME_5=44:55:66:77:88:99:smart-tv
```

## Complete .env File Example

Here's a complete `.env` file with Wi-Fi and MAC hostname configuration:

```bash
# Access Point Configuration
AP_SSID=rust-was-here
AP_PASS=change-me-for-your-own

# Station Mode Networks (for internet connectivity)
ST_SSID_1=HomeWifi
ST_PASS_1=homepassword123

ST_SSID_2=WorkWifi
ST_PASS_2=workpassword456

ST_SSID_3=GuestWifi
ST_PASS_3=guestpassword789

# Static MAC to Hostname Mappings
MAC_HOSTNAMES=aa:bb:cc:dd:ee:ff:my-laptop,11:22:33:44:55:66:raspberry-pi,77:88:99:aa:bb:cc:arduino-sensor,dd:ee:ff:11:22:33:security-camera,44:55:66:77:88:99:smart-tv,00:11:22:33:44:55:weather-station

# Alternative: Individual mappings (use either this OR MAC_HOSTNAMES above)
# MAC_HOSTNAME_1=aa:bb:cc:dd:ee:ff:my-laptop
# MAC_HOSTNAME_2=11:22:33:44:55:66:raspberry-pi
# MAC_HOSTNAME_3=77:88:99:aa:bb:cc:arduino-sensor
```

## Finding MAC Addresses

### From Connected Devices

Once devices are connected, the router logs will show their MAC addresses:

```
Client got IP 192.168.4.100 – MAC aa:bb:cc:dd:ee:ff – Hostname: ancient-waterfall.local
STA aa:bb:cc:dd:ee:ff (Ancient Waterfall) joined
```

### From Device Settings

#### Windows
```cmd
ipconfig /all
# Look for "Physical Address"
```

#### macOS/Linux
```bash
ifconfig
# Look for "ether" or "HWaddr"
```

#### Android
Settings → About Phone → Status → WiFi MAC Address

#### iOS
Settings → General → About → WiFi Address

## Hostname Rules

### Valid Hostnames
- Must be 1-63 characters long
- Can contain letters, numbers, and hyphens
- Cannot start or end with a hyphen
- Case insensitive (converted to lowercase)

### Examples
```
✅ Valid:
- my-laptop
- raspberry-pi-4b
- arduino01
- security-camera
- smart-tv-livingroom

❌ Invalid:
- -invalid (starts with hyphen)
- invalid- (ends with hyphen)
- too_long_hostname_that_exceeds_the_maximum_allowed_length_of_sixtythree_chars
- (empty string)
```

### Automatic Sanitization
The system automatically sanitizes hostnames:
- `My Device!` → `my-device`
- `Test_123` → `test-123`
- `Spaced Out Device` → `spaced-out-device`

## Device Examples

### Common Device Types

```bash
# Computers
MAC_HOSTNAMES=\
aa:bb:cc:dd:ee:f1:johns-laptop,\
aa:bb:cc:dd:ee:f2:maries-desktop,\
aa:bb:cc:dd:ee:f3:work-macbook

# IoT Devices
MAC_HOSTNAMES=\
11:22:33:44:55:01:raspberry-pi-main,\
11:22:33:44:55:02:arduino-temp-sensor,\
11:22:33:44:55:03:esp32-weather-station,\
11:22:33:44:55:04:security-camera-front,\
11:22:33:44:55:05:security-camera-back

# Smart Home
MAC_HOSTNAMES=\
77:88:99:aa:bb:01:smart-tv-living,\
77:88:99:aa:bb:02:smart-speaker-kitchen,\
77:88:99:aa:bb:03:thermostat-main,\
77:88:99:aa:bb:04:smart-bulb-bedroom,\
77:88:99:aa:bb:05:door-sensor-front
```

## Usage After Configuration

### Build and Flash
After updating your `.env` file:

```bash
# Rebuild to include new MAC mappings
just build
just flash
just run
```

### Accessing Configured Devices
Once configured devices connect, they'll use their static hostnames:

```bash
# Instead of dynamic names like:
ping ancient-waterfall.local

# You'll get your configured names:
ping my-laptop.local
ssh pi@raspberry-pi.local
curl http://arduino-sensor.local/data
```

## Monitoring

### Build-time Messages
During build, you'll see:
```
Loaded 5 static MAC hostname mappings from configuration
```

### Runtime Messages
When devices connect:
```
Using static hostname for MAC aa:bb:cc:dd:ee:ff: my-laptop.local
DNS: Registered my-laptop.local -> 192.168.4.100
```

### Status Reports
The router periodically reports registered hostnames:
```
🏠 Registered hostnames (5):
   my-laptop.local -> 192.168.4.100
   raspberry-pi.local -> 192.168.4.101
   arduino-sensor.local -> 192.168.4.102
   security-camera.local -> 192.168.4.103
   smart-tv.local -> 192.168.4.104
```

## Troubleshooting

### MAC Address Format
Ensure MAC addresses use colon format with lowercase hex:
```
✅ Correct: aa:bb:cc:dd:ee:ff
❌ Wrong: AA-BB-CC-DD-EE-FF
❌ Wrong: aabbccddeeff
❌ Wrong: aa.bb.cc.dd.ee.ff
```

### Duplicate Hostnames
The system prevents duplicate hostnames:
```
ERROR: Hostname 'my-device' is already reserved for MAC 11:22:33:44:55:66
```

### Invalid Configuration
Invalid entries are logged and skipped:
```
WARN: Invalid MAC hostname config entry: invalid-format
WARN: Invalid MAC address in config entry: gg:hh:ii:jj:kk:ll:device
```

### Build Issues
If configuration isn't loading:
1. Check `.env` file syntax
2. Ensure `cargo clean && cargo build`
3. Verify MAC address format

## Advanced Configuration

### Large Networks
For networks with many devices, use a separate configuration file:

1. Create `mac_mappings.txt`:
```
aa:bb:cc:dd:ee:01:laptop-user1
aa:bb:cc:dd:ee:02:laptop-user2
aa:bb:cc:dd:ee:03:desktop-user1
# ... more mappings
```

2. Load into `.env`:
```bash
MAC_HOSTNAMES=$(cat mac_mappings.txt | tr '\n' ',' | sed 's/,$//')
```

### Dynamic Updates
Currently, MAC mappings are loaded at build time. For runtime updates, you would need to:
1. Modify `.env` file
2. Rebuild and reflash the device

## Best Practices

### Naming Conventions
- Use consistent naming patterns
- Include device type: `raspberry-pi-`, `arduino-`, `camera-`
- Include location: `-kitchen`, `-bedroom`, `-garage`
- Use numbers for multiple similar devices: `-01`, `-02`

### Organization
```bash
# Computers
MAC_HOSTNAMES=\
aa:bb:cc:dd:ee:01:laptop-john,\
aa:bb:cc:dd:ee:02:desktop-marie,\
# IoT Sensors
11:22:33:44:55:01:temp-sensor-living,\
11:22:33:44:55:02:temp-sensor-bedroom,\
# Cameras
77:88:99:aa:bb:01:camera-front-door,\
77:88:99:aa:bb:02:camera-back-yard
```

### Security
- Don't include sensitive information in hostnames
- Use generic names for publicly visible devices
- Consider using device identifiers rather than personal names

This configuration system provides a flexible way to assign meaningful, permanent hostnames to your network devices while maintaining the convenience of automatic DNS resolution.