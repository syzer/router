# Integration Test Example: MAC Hostname DNS Server

This document provides a complete integration test example demonstrating the MAC hostname functionality in your ESP32 router.

## Test Scenario Setup

### .env Configuration
```bash
# Access Point Configuration
AP_SSID=test-router-wifi
AP_PASS=test123password

# Station Mode Network
ST_SSID_1=HomeWiFi
ST_PASS_1=homepassword123

# Static MAC to Hostname Mappings for Test Devices
MAC_HOSTNAMES=aa:bb:cc:dd:ee:ff:johns-laptop,11:22:33:44:55:66:raspberry-pi-test,77:88:99:aa:bb:cc:arduino-weather,dd:ee:ff:11:22:33:security-cam-demo,44:55:66:77:88:99:smart-tv-test
```

## Expected Boot Sequence

### 1. System Initialization
```
[INFO] .....Booting up Wi-Fi AP + STA bridge........
[INFO] Found 1 Wi-Fi networks configured for STA cycling
[INFO]   STA Network 1: HomeWiFi
[INFO] Loaded 5 static MAC hostname mappings from configuration
[INFO] Static MAC hostname mappings (5):
[INFO]   aa:bb:cc:dd:ee:ff -> johns-laptop.local
[INFO]   11:22:33:44:55:66 -> raspberry-pi-test.local
[INFO]   77:88:99:aa:bb:cc -> arduino-weather.local
[INFO]   dd:ee:ff:11:22:33 -> security-cam-demo.local
[INFO]   44:55:66:77:88:99 -> smart-tv-test.local
```

### 2. DNS Service Startup
```
[INFO] mDNS service initialized (local registry mode)
[INFO] DNS server service started (using mDNS for .local domains)
[INFO] DNS server started successfully
[INFO] DHCP configured to advertise 192.168.4.1 as DNS server
[INFO] NAPT enabled – AP clients have Internet!
```

### 3. Initial Status Report
```
[INFO] 🌐 DNS Server Configuration:
[INFO]    - mDNS service initialized and running
[INFO]    - Router hostname: esp-router.local
[INFO]    - DNS resolution enabled for .local domains
[INFO]    - DHCP clients will use router as DNS server
[INFO]    - Static MAC mappings: 5
```

## Device Connection Tests

### Test Case 1: Static MAC Mapping (Johns Laptop)
```
Device with MAC: aa:bb:cc:dd:ee:ff connects

Expected Log Output:
[INFO] Using static hostname for MAC aa:bb:cc:dd:ee:ff: johns-laptop.local
[INFO] DNS: Registered johns-laptop.local -> 192.168.4.100
[INFO] Device registered: MAC aa:bb:cc:dd:ee:ff -> johns-laptop.local (192.168.4.100)
[INFO] Client got IP 192.168.4.100 – MAC aa:bb:cc:dd:ee:ff – Hostname: johns-laptop.local
[INFO] STA aa:bb:cc:dd:ee:ff (johns-laptop) joined (RSSI will appear in 5s logger)
```

### Test Case 2: Another Static MAC Mapping (Raspberry Pi)
```
Device with MAC: 11:22:33:44:55:66 connects

Expected Log Output:
[INFO] Using static hostname for MAC 11:22:33:44:55:66: raspberry-pi-test.local
[INFO] DNS: Registered raspberry-pi-test.local -> 192.168.4.101
[INFO] Device registered: MAC 11:22:33:44:55:66 -> raspberry-pi-test.local (192.168.4.101)
[INFO] Client got IP 192.168.4.101 – MAC 11:22:33:44:55:66 – Hostname: raspberry-pi-test.local
[INFO] STA 11:22:33:44:55:66 (raspberry-pi-test) joined (RSSI will appear in 5s logger)
```

### Test Case 3: Unknown Device (Dynamic Hostname)
```
Device with MAC: 99:88:77:66:55:44 connects (not in static mappings)

Expected Log Output:
[INFO] DNS: Registered ancient-waterfall.local -> 192.168.4.102
[INFO] mDNS: Registered ancient-waterfall.local -> 192.168.4.102
[INFO] Client got IP 192.168.4.102 – MAC 99:88:77:66:55:44 – Hostname: ancient-waterfall.local
[INFO] STA 99:88:77:66:55:44 (Ancient Waterfall) joined (RSSI will appear in 5s logger)
```

## Periodic Status Reports

### Every 30 Seconds
```
[INFO] 🏠 Registered hostnames (8):
[INFO]    johns-laptop.local -> 192.168.4.100
[INFO]    raspberry-pi-test.local -> 192.168.4.101
[INFO]    ancient-waterfall.local -> 192.168.4.102
[INFO]    arduino-weather.local -> 192.168.4.103
[INFO]    security-cam-demo.local -> 192.168.4.104
[INFO]    smart-tv-test.local -> 192.168.4.105
[INFO]    brave-mountain.local -> 192.168.4.106
[INFO]    clever-river.local -> 192.168.4.107
```

### RSSI Distance Logging (Every 3 Seconds)
```
[INFO] 📶 RSSI -45 dBm → ≈2.1 m (client johns-laptop / aa:bb:cc:dd:ee:ff)
[INFO] 📶 RSSI -52 dBm → ≈4.2 m (client raspberry-pi-test / 11:22:33:44:55:66)
[INFO] 📶 RSSI -38 dBm → ≈1.3 m (client ancient-waterfall / 99:88:77:66:55:44)
[INFO] 📶 RSSI -61 dBm → ≈8.5 m (client arduino-weather / 77:88:99:aa:bb:cc)
```

## Client Testing

### From Connected Device - Test DNS Resolution

```bash
# Test 1: Resolve static hostname
$ nslookup johns-laptop.local 192.168.4.1
Server:    192.168.4.1
Address:   192.168.4.1#53

Non-authoritative answer:
Name:   johns-laptop.local
Address: 192.168.4.100

# Test 2: Resolve another static hostname
$ nslookup raspberry-pi-test.local 192.168.4.1
Server:    192.168.4.1
Address:   192.168.4.1#53

Non-authoritative answer:
Name:   raspberry-pi-test.local
Address: 192.168.4.101

# Test 3: Resolve dynamic hostname
$ nslookup ancient-waterfall.local 192.168.4.1
Server:    192.168.4.1
Address:   192.168.4.1#53

Non-authoritative answer:
Name:   ancient-waterfall.local
Address: 192.168.4.102
```

### Connectivity Tests

```bash
# Ping tests
$ ping johns-laptop.local
PING johns-laptop.local (192.168.4.100): 56 data bytes
64 bytes from 192.168.4.100: icmp_seq=0 ttl=64 time=2.145 ms
64 bytes from 192.168.4.100: icmp_seq=1 ttl=64 time=1.932 ms

$ ping raspberry-pi-test.local
PING raspberry-pi-test.local (192.168.4.101): 56 data bytes
64 bytes from 192.168.4.101: icmp_seq=0 ttl=64 time=3.421 ms
64 bytes from 192.168.4.101: icmp_seq=1 ttl=64 time=2.876 ms

# SSH test (if SSH is running on target device)
$ ssh pi@raspberry-pi-test.local
pi@192.168.4.101's password: 
Linux raspberrypi 5.15.61-v7l+ #1579

# HTTP test (if device has web interface)
$ curl http://arduino-weather.local/status
{"temperature": 23.5, "humidity": 65.2, "status": "ok"}
```

## Error Scenarios Testing

### Test Case 4: Duplicate Hostname Attempt
```
Try to add another device with same hostname in configuration:

.env addition:
MAC_HOSTNAMES=...,ff:ee:dd:cc:bb:aa:johns-laptop

Expected Build Error:
ERROR: Hostname 'johns-laptop' is already reserved for MAC aa:bb:cc:dd:ee:ff
```

### Test Case 5: Invalid MAC Address Format
```
Invalid .env entry:
MAC_HOSTNAMES=...,invalid-mac-format:my-device

Expected Build Warning:
WARN: Invalid MAC address in config entry: invalid-mac-format:my-device
```

### Test Case 6: Invalid Hostname
```
Invalid hostname in .env:
MAC_HOSTNAMES=...,aa:bb:cc:dd:ee:ff:-invalid-hostname-

Expected Build Warning:
WARN: Failed to add mapping for aa:bb:cc:dd:ee:ff:-invalid-hostname-: Invalid hostname: -invalid-hostname-
```

## Performance Validation

### Memory Usage Test
```
Expected Memory Footprint:
- Base DNS server: ~2KB
- Per device entry: ~100 bytes
- 10 devices: ~3KB total
- 50 devices: ~7KB total
- 100 devices: ~12KB total
```

### Response Time Test
```bash
# Measure DNS response time
$ time nslookup johns-laptop.local 192.168.4.1
# Expected: < 10ms response time

# Bulk ping test
$ for i in {1..100}; do ping -c 1 johns-laptop.local > /dev/null; done
# Expected: Consistent sub-5ms response times
```

## Integration with Existing Features

### Network Cycling Test
```
Press GPIO9 button to cycle networks:

Expected Log Output:
[INFO] 🔄 Button pressed - switching STA to network: WorkWiFi
[INFO] STA reconnect initiated
[INFO] Connecting STA to `WorkWiFi` …

# DNS service should continue working during network switch
# All registered devices should remain accessible
```

### LED Indicator Test
```
When new device connects:

Expected Behavior:
- LED blinks pink 5 times (200ms on/off cycle)
- DNS registration happens during blink sequence
- Device becomes accessible immediately after registration
```

## Troubleshooting Scenarios

### Scenario 1: DNS Not Resolving
```bash
# Step 1: Verify router connectivity
$ ping 192.168.4.1
# Should respond

# Step 2: Check if device got DHCP
$ ipconfig    # Windows
$ ifconfig    # macOS/Linux
# Should show 192.168.4.x IP and DNS server as 192.168.4.1

# Step 3: Manual DNS query
$ nslookup hostname.local 192.168.4.1
# Should return IP address

# Step 4: Check router logs for device registration
# Look for "DNS: Registered hostname.local -> IP" message
```

### Scenario 2: Static Mapping Not Working
```
Symptoms: Device gets dynamic hostname instead of static

Troubleshooting:
1. Check .env file MAC format (lowercase, colon-separated)
2. Verify build included the mapping (check build logs)
3. Confirm actual device MAC matches configuration
4. Check for typos in MAC address

Router logs should show:
[INFO] Using static hostname for MAC xx:xx:xx:xx:xx:xx: hostname.local
NOT:
[INFO] DNS: Registered random-name.local -> IP
```

## Success Criteria

### ✅ All Tests Pass When:

1. **Static MAC mappings work correctly**
   - Configured devices get assigned hostnames
   - Hostnames resolve to correct IP addresses
   - No conflicts or errors in logs

2. **Dynamic hostname generation works**
   - Unknown devices get friendly names
   - Names are from the generated pool
   - No duplicate assignments

3. **DNS resolution is fast and reliable**
   - Sub-10ms response times
   - 100% success rate for registered devices
   - Proper error handling for unknown hosts

4. **Integration maintains existing functionality**
   - Network cycling still works
   - LED indicators function
   - RSSI distance measurement continues
   - NAT/internet access preserved

5. **Error handling is robust**
   - Invalid configurations are rejected
   - Runtime errors don't crash the system
   - Clear error messages in logs

6. **Performance is acceptable**
   - Memory usage under 15KB for 100 devices
   - No noticeable delay in device registration
   - ESP32 remains responsive

## Complete Test Script Summary

```bash
# Build and deploy
just build
just flash
just run

# Monitor logs for:
# - Static mapping loading
# - DNS service startup
# - Device registrations (both static and dynamic)
# - Periodic status reports
# - RSSI measurements

# Test from client devices:
# - DNS resolution (nslookup)
# - Ping connectivity  
# - SSH/HTTP access via hostname
# - Performance measurements

# Verify error handling:
# - Invalid .env configurations
# - Duplicate hostname attempts
# - Network connectivity issues
```

This integration test validates that your ESP32 router successfully combines DNS server functionality with static MAC address hostname assignment while maintaining all existing features and performance characteristics.