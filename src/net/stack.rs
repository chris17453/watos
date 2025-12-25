//! Network Stack - Raw packet handling for ping/ARP
//!
//! Simple implementation using smoltcp wire types for packet construction

use super::e1000::E1000;
use alloc::vec;
use smoltcp::wire::{
    ArpHardware, ArpOperation, ArpPacket, ArpRepr,
    EthernetAddress, EthernetFrame, EthernetProtocol, EthernetRepr,
    Icmpv4Message, Icmpv4Packet, Icmpv4Repr,
    IpProtocol, Ipv4Address, Ipv4Packet, Ipv4Repr,
};
use smoltcp::phy::ChecksumCapabilities;

/// Network configuration
pub struct NetConfig {
    pub ip_addr: Ipv4Address,
    pub netmask: Ipv4Address,
    pub gateway: Ipv4Address,
    pub mac_addr: EthernetAddress,
}

impl Default for NetConfig {
    fn default() -> Self {
        // QEMU user-mode networking defaults
        Self {
            ip_addr: Ipv4Address::new(10, 0, 2, 15),
            netmask: Ipv4Address::new(255, 255, 255, 0),
            gateway: Ipv4Address::new(10, 0, 2, 2),
            mac_addr: EthernetAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]),
        }
    }
}

/// Simple network stack for ping and basic operations
pub struct NetworkStack {
    pub nic: E1000,
    pub config: NetConfig,
    arp_cache: [(Ipv4Address, EthernetAddress); 8],
    arp_cache_len: usize,
}

impl NetworkStack {
    pub fn new(nic: E1000) -> Self {
        let mac = nic.mac_address();
        let mut config = NetConfig::default();
        config.mac_addr = EthernetAddress(mac);

        Self {
            nic,
            config,
            arp_cache: [(Ipv4Address::new(0,0,0,0), EthernetAddress([0;6])); 8],
            arp_cache_len: 0,
        }
    }

    /// Lookup MAC address in ARP cache
    fn arp_lookup(&self, ip: Ipv4Address) -> Option<EthernetAddress> {
        for i in 0..self.arp_cache_len {
            if self.arp_cache[i].0 == ip {
                return Some(self.arp_cache[i].1);
            }
        }
        None
    }

    /// Add entry to ARP cache
    fn arp_add(&mut self, ip: Ipv4Address, mac: EthernetAddress) {
        // Check if already exists
        for i in 0..self.arp_cache_len {
            if self.arp_cache[i].0 == ip {
                self.arp_cache[i].1 = mac;
                return;
            }
        }
        // Add new entry
        if self.arp_cache_len < 8 {
            self.arp_cache[self.arp_cache_len] = (ip, mac);
            self.arp_cache_len += 1;
        }
    }

    /// Send an ARP request
    fn send_arp_request(&mut self, target_ip: Ipv4Address) {
        let arp_repr = ArpRepr::EthernetIpv4 {
            operation: ArpOperation::Request,
            source_hardware_addr: self.config.mac_addr,
            source_protocol_addr: self.config.ip_addr,
            target_hardware_addr: EthernetAddress([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), // Unknown - asking for this
            target_protocol_addr: target_ip,
        };

        let eth_repr = EthernetRepr {
            src_addr: self.config.mac_addr,
            dst_addr: EthernetAddress::BROADCAST,
            ethertype: EthernetProtocol::Arp,
        };

        let mut buffer = vec![0u8; 14 + 28]; // Ethernet + ARP
        let mut eth_frame = EthernetFrame::new_unchecked(&mut buffer);
        eth_repr.emit(&mut eth_frame);

        let mut arp_packet = ArpPacket::new_unchecked(eth_frame.payload_mut());
        arp_repr.emit(&mut arp_packet);

        self.nic.send(&buffer);
    }

    /// Process received packet
    fn process_packet(&mut self, data: &[u8]) -> Option<(Ipv4Address, u16, u8)> {
        if data.len() < 14 {
            return None;
        }

        let eth_frame = EthernetFrame::new_checked(data).ok()?;

        match eth_frame.ethertype() {
            EthernetProtocol::Arp => {
                let arp = ArpPacket::new_checked(eth_frame.payload()).ok()?;
                if let Ok(arp_repr) = ArpRepr::parse(&arp) {
                    if let ArpRepr::EthernetIpv4 {
                        operation: ArpOperation::Reply,
                        source_hardware_addr,
                        source_protocol_addr,
                        ..
                    } = arp_repr {
                        self.arp_add(source_protocol_addr, source_hardware_addr);
                    }
                    // Handle ARP request for our IP
                    if let ArpRepr::EthernetIpv4 {
                        operation: ArpOperation::Request,
                        source_hardware_addr,
                        source_protocol_addr,
                        target_protocol_addr,
                        ..
                    } = arp_repr {
                        if target_protocol_addr == self.config.ip_addr {
                            self.send_arp_reply(source_hardware_addr, source_protocol_addr);
                        }
                    }
                }
            }
            EthernetProtocol::Ipv4 => {
                let ip_packet = Ipv4Packet::new_checked(eth_frame.payload()).ok()?;
                if ip_packet.dst_addr() != self.config.ip_addr {
                    return None;
                }

                if ip_packet.next_header() == IpProtocol::Icmp {
                    let icmp = Icmpv4Packet::new_checked(ip_packet.payload()).ok()?;
                    if icmp.msg_type() == Icmpv4Message::EchoReply {
                        let seq = icmp.echo_seq_no();
                        let ttl = ip_packet.hop_limit();
                        return Some((ip_packet.src_addr(), seq, ttl));
                    }
                }
            }
            _ => {}
        }

        None
    }

    /// Send ARP reply
    fn send_arp_reply(&mut self, target_mac: EthernetAddress, target_ip: Ipv4Address) {
        let arp_repr = ArpRepr::EthernetIpv4 {
            operation: ArpOperation::Reply,
            source_hardware_addr: self.config.mac_addr,
            source_protocol_addr: self.config.ip_addr,
            target_hardware_addr: target_mac,
            target_protocol_addr: target_ip,
        };

        let eth_repr = EthernetRepr {
            src_addr: self.config.mac_addr,
            dst_addr: target_mac,
            ethertype: EthernetProtocol::Arp,
        };

        let mut buffer = vec![0u8; 14 + 28];
        let mut eth_frame = EthernetFrame::new_unchecked(&mut buffer);
        eth_repr.emit(&mut eth_frame);

        let mut arp_packet = ArpPacket::new_unchecked(eth_frame.payload_mut());
        arp_repr.emit(&mut arp_packet);

        self.nic.send(&buffer);
    }

    /// Send ICMP echo request
    fn send_ping(&mut self, target_ip: Ipv4Address, target_mac: EthernetAddress, seq: u16) {
        let icmp_repr = Icmpv4Repr::EchoRequest {
            ident: 0x1234,
            seq_no: seq,
            data: b"WATOS",
        };

        let icmp_len = icmp_repr.buffer_len();
        let ip_len = 20 + icmp_len;
        let total_len = 14 + ip_len;

        let mut buffer = vec![0u8; total_len];

        // Build Ethernet frame
        let eth_repr = EthernetRepr {
            src_addr: self.config.mac_addr,
            dst_addr: target_mac,
            ethertype: EthernetProtocol::Ipv4,
        };
        let mut eth_frame = EthernetFrame::new_unchecked(&mut buffer);
        eth_repr.emit(&mut eth_frame);

        // Build IP packet
        let ip_repr = Ipv4Repr {
            src_addr: self.config.ip_addr,
            dst_addr: target_ip,
            next_header: IpProtocol::Icmp,
            payload_len: icmp_len,
            hop_limit: 64,
        };
        let mut ip_packet = Ipv4Packet::new_unchecked(eth_frame.payload_mut());
        ip_repr.emit(&mut ip_packet, &ChecksumCapabilities::default());

        // Build ICMP packet
        let mut icmp_packet = Icmpv4Packet::new_unchecked(ip_packet.payload_mut());
        icmp_repr.emit(&mut icmp_packet, &ChecksumCapabilities::default());

        self.nic.send(&buffer);
    }

    /// Ping a host - returns (success, seq, ttl) or timeout
    pub fn ping(&mut self, target: Ipv4Address) -> PingResult {
        // Special case: pinging ourselves
        if target == self.config.ip_addr {
            return PingResult::Success { seq: 1, ttl: 64 };
        }

        // Enable timer for timing operations
        crate::interrupts::enable_timer();

        // Determine if target is on local network or needs gateway
        let dest_ip = if self.is_local(target) {
            target
        } else {
            self.config.gateway
        };

        // Get MAC address via ARP if not cached
        let dest_mac = if let Some(mac) = self.arp_lookup(dest_ip) {
            mac
        } else {
            // Send ARP requests with retries
            let mut found_mac = None;
            let mut rx_buf = [0u8; 2048];

            for _retry in 0..3 {
                // Send ARP request
                self.send_arp_request(dest_ip);

                // Wait for ARP reply (~1 second = 18 ticks)
                let start = crate::interrupts::get_ticks();
                while crate::interrupts::get_ticks().wrapping_sub(start) < 18 {
                    let len = self.nic.recv(&mut rx_buf);
                    if len > 0 {
                        self.process_packet(&rx_buf[..len]);
                        if let Some(mac) = self.arp_lookup(dest_ip) {
                            found_mac = Some(mac);
                            break;
                        }
                    }
                    crate::interrupts::halt();
                }

                if found_mac.is_some() {
                    break;
                }
            }

            match found_mac {
                Some(mac) => mac,
                None => {
                    crate::interrupts::disable_timer();
                    return PingResult::Unreachable;
                }
            }
        };

        // Send ping
        let seq = 1u16;
        self.send_ping(target, dest_mac, seq);

        // Wait for reply (~2 seconds = 36 ticks)
        let mut rx_buf = [0u8; 2048];
        let start = crate::interrupts::get_ticks();
        while crate::interrupts::get_ticks().wrapping_sub(start) < 36 {
            let len = self.nic.recv(&mut rx_buf);
            if len > 0 {
                if let Some((src, reply_seq, ttl)) = self.process_packet(&rx_buf[..len]) {
                    if src == target && reply_seq == seq {
                        crate::interrupts::disable_timer();
                        return PingResult::Success { seq, ttl };
                    }
                }
            }
            crate::interrupts::halt();
        }

        crate::interrupts::disable_timer();
        PingResult::Timeout
    }

    /// Check if IP is on local network
    fn is_local(&self, ip: Ipv4Address) -> bool {
        let ip_bytes = ip.as_bytes();
        let my_bytes = self.config.ip_addr.as_bytes();
        let mask_bytes = self.config.netmask.as_bytes();

        for i in 0..4 {
            if (ip_bytes[i] & mask_bytes[i]) != (my_bytes[i] & mask_bytes[i]) {
                return false;
            }
        }
        true
    }

    /// Get IP address as string bytes
    pub fn ip_string(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        let bytes = self.config.ip_addr.as_bytes();
        let mut pos = 0;
        for (i, &b) in bytes.iter().enumerate() {
            pos += write_u8(&mut buf[pos..], b);
            if i < 3 {
                buf[pos] = b'.';
                pos += 1;
            }
        }
        buf
    }
}

/// Result of a ping operation
#[derive(Debug, Clone, Copy)]
pub enum PingResult {
    Success { seq: u16, ttl: u8 },
    Timeout,
    Unreachable,
}

/// Parse an IPv4 address from a string
pub fn parse_ipv4(s: &str) -> Option<Ipv4Address> {
    let mut parts = s.split('.');
    let a: u8 = parts.next()?.parse().ok()?;
    let b: u8 = parts.next()?.parse().ok()?;
    let c: u8 = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Ipv4Address::new(a, b, c, d))
}

/// Write u8 as decimal to buffer, return bytes written
fn write_u8(buf: &mut [u8], val: u8) -> usize {
    if val >= 100 {
        buf[0] = b'0' + val / 100;
        buf[1] = b'0' + (val / 10) % 10;
        buf[2] = b'0' + val % 10;
        3
    } else if val >= 10 {
        buf[0] = b'0' + val / 10;
        buf[1] = b'0' + val % 10;
        2
    } else {
        buf[0] = b'0' + val;
        1
    }
}
