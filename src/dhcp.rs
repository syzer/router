use crate::{format_mac, STATIC_LEASES};
use anyhow::{anyhow, Context, Result};
use esp_idf_sys as sys;
use libc::{calloc, free};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::os::raw::{c_char, c_void};
use std::ptr::{self, NonNull};

pub const DYNAMIC_POOL_START: u32 = 2;
pub const DYNAMIC_POOL_END: u32 = 99;
pub const STATIC_POOL_START: u32 = 100;
const STATIC_LEASE_HOLD_SECONDS: u32 = u32::MAX / 2;

pub static DHCP_RESERVATIONS: Lazy<HashMap<[u8; 6], Ipv4Addr>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for lease in STATIC_LEASES {
        map.insert(
            lease.mac,
            Ipv4Addr::new(lease.ip[0], lease.ip[1], lease.ip[2], lease.ip[3]),
        );
    }
    map
});

#[repr(C)]
#[derive(Clone, Copy)]
struct DhcpsLease {
    enable: bool,
    start_ip: sys::ip4_addr_t,
    end_ip: sys::ip4_addr_t,
}

impl DhcpsLease {
    fn from_range(start: Ipv4Addr, end: Ipv4Addr) -> Self {
        Self {
            enable: true,
            start_ip: ip4_from_ipv4(start),
            end_ip: ip4_from_ipv4(end),
        }
    }
}

#[repr(C)]
struct DhcpsPool {
    ip: sys::ip4_addr_t,
    mac: [u8; 6],
    lease_timer: u32,
}

#[repr(C)]
struct ListNode {
    pnode: *mut DhcpsPool,
    pnext: *mut ListNode,
}

#[repr(C)]
struct Dhcps {
    dhcps_netif: *mut sys::netif,
    broadcast_dhcps: sys::ip4_addr_t,
    server_address: sys::ip4_addr_t,
    dns_server: sys::ip4_addr_t,
    client_address: sys::ip4_addr_t,
    client_address_plus: sys::ip4_addr_t,
    dhcps_mask: sys::ip4_addr_t,
    plist: *mut ListNode,
    renew: bool,
    dhcps_poll: DhcpsLease,
    dhcps_lease_time: u32,
    dhcps_offer: u8,
    dhcps_dns: u8,
    dhcps_captiveportal_uri: *mut c_char,
    dhcps_cb: Option<unsafe extern "C" fn(*mut c_void, *mut u8, *mut u8)>,
    dhcps_cb_arg: *mut c_void,
    dhcps_pcb: *mut c_void,
    state: i32,
}

#[repr(C)]
struct NetifRelatedData {
    is_point2point: bool,
    _reserved: [u8; 3],
    netif_type: i32,
}

#[repr(C)]
struct EspNetifObjHead {
    mac: [u8; 6],
    _mac_padding: [u8; 2],
    ip_info: *mut sys::esp_netif_ip_info_t,
    ip_info_old: *mut sys::esp_netif_ip_info_t,
    lwip_netif: *mut sys::netif,
    lwip_init_fn: Option<unsafe extern "C" fn(*mut sys::netif) -> i8>,
    lwip_input_fn: Option<
        unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *mut c_void) -> sys::esp_err_t,
    >,
    netif_handle: *mut c_void,
    related_data: *mut NetifRelatedData,
    dhcps: *mut Dhcps,
}

pub struct DhcpServerState {
    dhcps: NonNull<Dhcps>,
    network_base: u32,
    netmask: u32,
    dynamic_start: u32,
    dynamic_end: u32,
}

unsafe impl Send for DhcpServerState {}
unsafe impl Sync for DhcpServerState {}

impl DhcpServerState {
    pub fn new(handle: *mut sys::esp_netif_t) -> Result<Self> {
        let ip_info = fetch_ip_info(handle)?;

        let ip_host = u32::from_be(ip_info.ip.addr);
        let netmask_host = u32::from_be(ip_info.netmask.addr);
        let network_base = ip_host & netmask_host;

        anyhow::ensure!(
            STATIC_POOL_START > DYNAMIC_POOL_START,
            "Static pool start must be greater than dynamic pool start"
        );

        let pool_start_ip = host_to_ipv4(network_base + DYNAMIC_POOL_START);
        let pool_end_ip = host_to_ipv4(network_base + DYNAMIC_POOL_END);
        let lease = DhcpsLease::from_range(pool_start_ip, pool_end_ip);
        set_dhcp_lease(handle, &lease)?;

        let netif_obj = unsafe { &mut *(handle as *mut EspNetifObjHead) };
        let dhcps_ptr = NonNull::new(netif_obj.dhcps).context("DHCP server unavailable")?;

        let state = Self {
            dhcps: dhcps_ptr,
            network_base,
            netmask: netmask_host,
            dynamic_start: DYNAMIC_POOL_START,
            dynamic_end: DYNAMIC_POOL_END,
        };

        let max_host = state.install_static_leases(STATIC_LEASES)?;
        state.extend_pool_end(max_host);
        state.clamp_dynamic_cursor();

        Ok(state)
    }

    pub fn clamp_dynamic_cursor(&self) {
        unsafe {
            let dhcps = self.dhcps.as_ptr();
            let current_host = host_component((*dhcps).client_address_plus.addr, self.network_base);
            if current_host < self.dynamic_start || current_host > self.dynamic_end {
                let addr = host_to_be(self.network_base + self.dynamic_start);
                (*dhcps).client_address_plus.addr = addr;
            }
        }
    }

    pub fn touch_static_lease(&self, mac: &[u8; 6]) {
        unsafe {
            let mut node = (*self.dhcps.as_ptr()).plist;
            while !node.is_null() {
                let pool = (*node).pnode;
                if !pool.is_null() && (*pool).mac == *mac {
                    (*pool).lease_timer = STATIC_LEASE_HOLD_SECONDS;
                    break;
                }
                node = (*node).pnext;
            }
        }
    }

    fn install_static_leases(&self, reservations: &[crate::StaticLease]) -> Result<u32> {
        let mut max_host = DYNAMIC_POOL_END;
        for lease in reservations {
            let ip_host = u32::from_be_bytes(lease.ip);
            ensure_same_subnet(ip_host, self.network_base, self.netmask, lease.mac)?;

            let host_part = ip_host - self.network_base;
            anyhow::ensure!(
                host_part >= STATIC_POOL_START,
                "Static reservation {} for {} must be >= {}",
                host_to_ipv4(ip_host),
                format_mac(&lease.mac),
                host_to_ipv4(self.network_base + STATIC_POOL_START)
            );

            if host_part > max_host {
                max_host = host_part;
            }

            self.upsert_static_entry(lease.mac, ip_host)?;
        }
        Ok(max_host)
    }

    fn upsert_static_entry(&self, mac: [u8; 6], ip_host: u32) -> Result<()> {
        unsafe {
            let dhcps = self.dhcps.as_ptr();
            let ip_be = host_to_be(ip_host);
            let mut prev: *mut ListNode = ptr::null_mut();
            let mut current = (*dhcps).plist;

            while !current.is_null() {
                let pool = (*current).pnode;
                if pool.is_null() {
                    prev = current;
                    current = (*current).pnext;
                    continue;
                }

                let current_ip_host = u32::from_be((*pool).ip.addr);

                if (*pool).mac == mac {
                    (*pool).ip.addr = ip_be;
                    (*pool).lease_timer = STATIC_LEASE_HOLD_SECONDS;
                    return Ok(());
                }

                if current_ip_host == ip_host {
                    (*pool).mac = mac;
                    (*pool).lease_timer = STATIC_LEASE_HOLD_SECONDS;
                    return Ok(());
                }

                if current_ip_host > ip_host {
                    break;
                }

                prev = current;
                current = (*current).pnext;
            }

            let pool_ptr = calloc(1, std::mem::size_of::<DhcpsPool>()) as *mut DhcpsPool;
            if pool_ptr.is_null() {
                return Err(anyhow!("Failed to allocate DHCP static pool entry"));
            }

            (*pool_ptr).ip.addr = ip_be;
            (*pool_ptr).mac = mac;
            (*pool_ptr).lease_timer = STATIC_LEASE_HOLD_SECONDS;

            let node_ptr = calloc(1, std::mem::size_of::<ListNode>()) as *mut ListNode;
            if node_ptr.is_null() {
                free(pool_ptr as *mut c_void);
                return Err(anyhow!("Failed to allocate DHCP list node"));
            }

            (*node_ptr).pnode = pool_ptr;
            (*node_ptr).pnext = current;

            if prev.is_null() {
                (*dhcps).plist = node_ptr;
            } else {
                (*prev).pnext = node_ptr;
            }
        }

        Ok(())
    }

    fn extend_pool_end(&self, max_host: u32) {
        unsafe {
            let dhcps = self.dhcps.as_ptr();
            let target_host = max_host.max(self.dynamic_end);
            (*dhcps).dhcps_poll.end_ip.addr =
                host_to_be(self.network_base.saturating_add(target_host));
        }
    }
}

fn ensure_same_subnet(
    ip_host: u32,
    network_base: u32,
    netmask_host: u32,
    mac: [u8; 6],
) -> Result<u32> {
    if (ip_host & netmask_host) != network_base {
        Err(anyhow!(
            "Static lease {} for {} outside AP subnet",
            host_to_ipv4(ip_host),
            format_mac(&mac)
        ))
    } else {
        Ok(ip_host)
    }
}

fn fetch_ip_info(handle: *mut sys::esp_netif_t) -> Result<sys::esp_netif_ip_info_t> {
    let mut info = sys::esp_netif_ip_info_t {
        ip: sys::esp_ip4_addr_t { addr: 0 },
        netmask: sys::esp_ip4_addr_t { addr: 0 },
        gw: sys::esp_ip4_addr_t { addr: 0 },
    };
    let err = unsafe { sys::esp_netif_get_ip_info(handle, &mut info) };
    if err == sys::ESP_OK {
        Ok(info)
    } else {
        Err(esp_error(err))
    }
}

fn set_dhcp_lease(handle: *mut sys::esp_netif_t, lease: &DhcpsLease) -> Result<()> {
    let stop_err = unsafe { sys::esp_netif_dhcps_stop(handle) };
    if stop_err != sys::ESP_OK && stop_err != sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STOPPED {
        return Err(esp_error(stop_err));
    }

    let mut lease_copy = *lease;
    let set_err = unsafe {
        sys::esp_netif_dhcps_option(
            handle,
            sys::esp_netif_dhcp_option_mode_t_ESP_NETIF_OP_SET,
            sys::esp_netif_dhcp_option_id_t_ESP_NETIF_REQUESTED_IP_ADDRESS,
            &mut lease_copy as *mut _ as *mut c_void,
            core::mem::size_of::<DhcpsLease>() as u32,
        )
    };

    if set_err != sys::ESP_OK {
        return Err(esp_error(set_err));
    }

    let start_err = unsafe { sys::esp_netif_dhcps_start(handle) };
    if start_err != sys::ESP_OK && start_err != sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STARTED {
        return Err(esp_error(start_err));
    }

    Ok(())
}

fn ip4_from_ipv4(ip: Ipv4Addr) -> sys::ip4_addr_t {
    sys::ip4_addr_t {
        addr: host_to_be(ipv4_to_host(ip)),
    }
}

fn host_to_ipv4(host: u32) -> Ipv4Addr {
    Ipv4Addr::from(host_to_be(host))
}

fn ipv4_to_host(ip: Ipv4Addr) -> u32 {
    u32::from_be(u32::from(ip))
}

fn host_to_be(host: u32) -> u32 {
    host.to_be()
}

fn host_component(addr_be: u32, network_base: u32) -> u32 {
    let addr_host = u32::from_be(addr_be);
    addr_host.saturating_sub(network_base)
}

fn esp_error(err: i32) -> anyhow::Error {
    let name = unsafe { std::ffi::CStr::from_ptr(sys::esp_err_to_name(err)) }.to_string_lossy();
    anyhow!("ESP error {err}: {name}")
}
