//! Network Stack - Raw packet handling for ping/ARP
//!
//! Simple implementation using smoltcp wire types for packet construction.

#![no_std]

extern crate alloc;

use alloc::vec;
use smoltcp::wire::{
    ArpOperation, ArpPacket, ArpRepr,
    EthernetAddress, EthernetFrame, EthernetProtocol, EthernetRepr,
    Icmpv4Message, Icmpv4Packet, Icmpv4Repr,
    IpProtocol, Ipv4Address, Ipv4Packet, Ipv4Repr,
};
use smoltcp::phy::ChecksumCapabilities;

/// Trait for network driver access
pub trait NetworkDriver {
    fn get_mac_address(&self) -> Option<[u8; 6]>;
    fn send_packet(&self, data: &[u8]) -> Result<(), ()>;
    fn receive_packet(&self, buffer: &mut [u8]) -> Option<usize>;
    fn get_ticks(&self) -> u64;
    fn halt(&self);
    fn enable_timer(&self);
    fn disable_timer(&self);
}

/// Network configuration
pub struct NetConfig {
    pub ip_addr: Ipv4Address,
    pub netmask: Ipv4Address,
    pub gateway: Ipv4Address,
    pub mac_addr: EthernetAddress,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            ip_addr: Ipv4Address::new(10, 0, 2, 15),
            netmask: Ipv4Address::new(255, 255, 255, 0),
            gateway: Ipv4Address::new(10, 0, 2, 2),
            mac_addr: EthernetAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]),
        }
    }
}

/// Simple network stack for ping and basic operations
pub struct NetworkStack<D: NetworkDriver> {
    pub config: NetConfig,
    arp_cache: [(Ipv4Address, EthernetAddress); 8],
    arp_cache_len: usize,
    driver: D,
}

impl<D: NetworkDriver> NetworkStack<D> {
    /// Create a new network stack
    pub fn new(driver: D) -> Option<Self> {
        let mac = driver.get_mac_address()?;
        let mut config = NetConfig::default();
        config.mac_addr = EthernetAddress(mac);

        Some(Self {
            config,
            arp_cache: [(Ipv4Address::new(0, 0, 0, 0), EthernetAddress([0; 6])); 8],
            arp_cache_len: 0,
            driver,
        })
    }

    fn send(&self, buffer: &[u8]) {
        let _ = self.driver.send_packet(buffer);
    }

    fn recv(&self, buffer: &mut [u8]) -> usize {
        self.driver.receive_packet(buffer).unwrap_or(0)
    }

    fn arp_lookup(&self, ip: Ipv4Address) -> Option<EthernetAddress> {
        for i in 0..self.arp_cache_len {
            if self.arp_cache[i].0 == ip {
                return Some(self.arp_cache[i].1);
            }
        }
        None
    }

    fn arp_add(&mut self, ip: Ipv4Address, mac: EthernetAddress) {
        for i in 0..self.arp_cache_len {
            if self.arp_cache[i].0 == ip {
                self.arp_cache[i].1 = mac;
                return;
            }
        }
        if self.arp_cache_len < 8 {
            self.arp_cache[self.arp_cache_len] = (ip, mac);
            self.arp_cache_len += 1;
        }
    }

    fn send_arp_request(&mut self, target_ip: Ipv4Address) {
        let arp_repr = ArpRepr::EthernetIpv4 {
            operation: ArpOperation::Request,
            source_hardware_addr: self.config.mac_addr,
            source_protocol_addr: self.config.ip_addr,
            target_hardware_addr: EthernetAddress([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            target_protocol_addr: target_ip,
        };

        let eth_repr = EthernetRepr {
            src_addr: self.config.mac_addr,
            dst_addr: EthernetAddress::BROADCAST,
            ethertype: EthernetProtocol::Arp,
        };

        let mut buffer = vec![0u8; 14 + 28];
        let mut eth_frame = EthernetFrame::new_unchecked(&mut buffer);
        eth_repr.emit(&mut eth_frame);

        let mut arp_packet = ArpPacket::new_unchecked(eth_frame.payload_mut());
        arp_repr.emit(&mut arp_packet);

        self.send(&buffer);
    }

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
                    } = arp_repr
                    {
                        self.arp_add(source_protocol_addr, source_hardware_addr);
                    }
                    if let ArpRepr::EthernetIpv4 {
                        operation: ArpOperation::Request,
                        source_hardware_addr,
                        source_protocol_addr,
                        target_protocol_addr,
                        ..
                    } = arp_repr
                    {
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

        self.send(&buffer);
    }

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

        let eth_repr = EthernetRepr {
            src_addr: self.config.mac_addr,
            dst_addr: target_mac,
            ethertype: EthernetProtocol::Ipv4,
        };
        let mut eth_frame = EthernetFrame::new_unchecked(&mut buffer);
        eth_repr.emit(&mut eth_frame);

        let ip_repr = Ipv4Repr {
            src_addr: self.config.ip_addr,
            dst_addr: target_ip,
            next_header: IpProtocol::Icmp,
            payload_len: icmp_len,
            hop_limit: 64,
        };
        let mut ip_packet = Ipv4Packet::new_unchecked(eth_frame.payload_mut());
        ip_repr.emit(&mut ip_packet, &ChecksumCapabilities::default());

        let mut icmp_packet = Icmpv4Packet::new_unchecked(ip_packet.payload_mut());
        icmp_repr.emit(&mut icmp_packet, &ChecksumCapabilities::default());

        self.send(&buffer);
    }

    /// Ping a host
    pub fn ping(&mut self, target: Ipv4Address) -> PingResult {
        if target == self.config.ip_addr {
            return PingResult::Success { seq: 1, ttl: 64 };
        }

        self.driver.enable_timer();

        let dest_ip = if self.is_local(target) {
            target
        } else {
            self.config.gateway
        };

        let dest_mac = if let Some(mac) = self.arp_lookup(dest_ip) {
            mac
        } else {
            let mut found_mac = None;
            let mut rx_buf = [0u8; 2048];

            for _retry in 0..3 {
                self.send_arp_request(dest_ip);

                let start = self.driver.get_ticks();
                while self.driver.get_ticks().wrapping_sub(start) < 18 {
                    let len = self.recv(&mut rx_buf);
                    if len > 0 {
                        self.process_packet(&rx_buf[..len]);
                        if let Some(mac) = self.arp_lookup(dest_ip) {
                            found_mac = Some(mac);
                            break;
                        }
                    }
                    self.driver.halt();
                }

                if found_mac.is_some() {
                    break;
                }
            }

            match found_mac {
                Some(mac) => mac,
                None => {
                    self.driver.disable_timer();
                    return PingResult::Unreachable;
                }
            }
        };

        let seq = 1u16;
        self.send_ping(target, dest_mac, seq);

        let mut rx_buf = [0u8; 2048];
        let start = self.driver.get_ticks();
        while self.driver.get_ticks().wrapping_sub(start) < 36 {
            let len = self.recv(&mut rx_buf);
            if len > 0 {
                if let Some((src, reply_seq, ttl)) = self.process_packet(&rx_buf[..len]) {
                    if src == target && reply_seq == seq {
                        self.driver.disable_timer();
                        return PingResult::Success { seq, ttl };
                    }
                }
            }
            self.driver.halt();
        }

        self.driver.disable_timer();
        PingResult::Timeout
    }

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
