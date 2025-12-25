# DOS64 Network Support Roadmap

## Current State
- QEMU configured with virtio-net device
- Port forwarding set up: 8080→80 (HTTP), 2323→23 (Telnet)
- Kernel has no network driver yet

## Implementation Plan

### Phase 1: Virtio-Net Driver
The virtio-net device is the most efficient way to do networking in QEMU.

**Files to create:**
- `src/net/mod.rs` - Network subsystem
- `src/net/virtio.rs` - Virtio-net driver
- `src/net/ethernet.rs` - Ethernet frame handling

**Key tasks:**
1. PCI device enumeration (find virtio-net device)
2. Virtio queue setup (TX/RX ring buffers)
3. MAC address configuration
4. Packet send/receive

### Phase 2: TCP/IP Stack
Use `smoltcp` crate - a lightweight, no_std TCP/IP stack.

```toml
# Add to Cargo.toml
smoltcp = { version = "0.11", default-features = false, features = ["medium-ethernet", "proto-ipv4", "socket-tcp", "socket-udp"] }
```

**Provides:**
- Ethernet frame parsing
- ARP (address resolution)
- IPv4
- TCP/UDP sockets
- DHCP client

### Phase 3: Simple Services

**Telnet Server (Port 23)**
- Accept connections
- Bridge to kernel console
- Remote command execution

**HTTP Server (Port 80)**
- Basic HTTP/1.1
- Serve static responses
- System status API

### Phase 4: Applications

**Potential apps:**
- `wget` - Fetch URLs
- `ping` - ICMP echo
- `netstat` - Connection status
- `httpd` - Web server daemon

## Architecture

```
┌─────────────────────────────────────────┐
│            Applications                  │
│   (httpd, telnetd, wget, ping)          │
├─────────────────────────────────────────┤
│         Socket API                       │
│   (tcp_connect, tcp_listen, send, recv) │
├─────────────────────────────────────────┤
│         smoltcp TCP/IP Stack            │
│   (TCP, UDP, IP, ARP, DHCP)             │
├─────────────────────────────────────────┤
│         Ethernet Driver                  │
│   (frame TX/RX, MAC handling)           │
├─────────────────────────────────────────┤
│         Virtio-Net Driver               │
│   (PCI, virtqueues, DMA)                │
├─────────────────────────────────────────┤
│         QEMU virtio-net-pci             │
└─────────────────────────────────────────┘
```

## Quick Start Code

### PCI Enumeration (find virtio-net)
```rust
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000; // Legacy, or 0x1041 for modern

fn find_virtio_net() -> Option<(u8, u8, u8)> {
    for bus in 0..256u16 {
        for device in 0..32u8 {
            for func in 0..8u8 {
                let vendor = pci_read_config_word(bus as u8, device, func, 0);
                let dev_id = pci_read_config_word(bus as u8, device, func, 2);
                if vendor == VIRTIO_VENDOR_ID && dev_id == VIRTIO_NET_DEVICE_ID {
                    return Some((bus as u8, device, func));
                }
            }
        }
    }
    None
}
```

### Simple Packet Send (pseudo-code)
```rust
fn send_packet(data: &[u8]) {
    // 1. Get free TX descriptor
    let desc_idx = virtqueue_get_free_desc(&TX_QUEUE);

    // 2. Set up descriptor pointing to packet data
    TX_DESCS[desc_idx].addr = data.as_ptr() as u64;
    TX_DESCS[desc_idx].len = data.len() as u32;
    TX_DESCS[desc_idx].flags = 0;

    // 3. Add to available ring
    TX_QUEUE.available.ring[TX_QUEUE.available.idx] = desc_idx;
    TX_QUEUE.available.idx += 1;

    // 4. Notify device
    outw(VIRTIO_QUEUE_NOTIFY, 1); // TX queue
}
```

## Container Networking

QEMU user-mode networking provides:
- Automatic NAT to host network
- DHCP server (guest gets 10.0.2.15)
- DNS forwarding
- Port forwarding (configured in Dockerfile)

Guest sees:
- IP: 10.0.2.15/24
- Gateway: 10.0.2.2
- DNS: 10.0.2.3

## Testing Network

Once driver is implemented:
```bash
# Build and run in container
./scripts/container.sh run

# In another terminal, test HTTP:
curl http://localhost:8080

# Test telnet:
telnet localhost 2323
```

## Dependencies

Add to Cargo.toml:
```toml
[dependencies]
smoltcp = { version = "0.11", default-features = false, features = [
    "medium-ethernet",
    "proto-ipv4",
    "socket-tcp",
    "socket-udp",
    "socket-dhcpv4"
]}
```

## Estimated Effort

| Phase | Complexity | Description |
|-------|------------|-------------|
| 1. Virtio driver | Medium | PCI + virtqueue setup |
| 2. TCP/IP stack | Easy | smoltcp integration |
| 3. Services | Easy | Telnet/HTTP servers |
| 4. Apps | Variable | Depends on scope |

## Next Steps

1. Start with PCI enumeration to detect virtio-net
2. Implement basic virtqueue handling
3. Integrate smoltcp for TCP/IP
4. Build simple telnet server for remote console
