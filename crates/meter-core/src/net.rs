use std::net::{Ipv4Addr, SocketAddrV4};

pub const LINKTYPE_NULL: i32 = 0;
pub const LINKTYPE_ETHERNET: i32 = 1;
pub const LINKTYPE_RAW: i32 = 101;
pub const LINKTYPE_LOOP: i32 = 108;

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;

const ETHERTYPE_OFFSET: usize = 12;
const ETHERNET_HEADER_LEN: usize = 14;
const VLAN_ETHERTYPE_OFFSET: usize = 16;
const VLAN_ETHERNET_HEADER_LEN: usize = 18;
const ETHERTYPE_VLAN: u16 = 0x8100;
const ETHERTYPE_IPV4: u16 = 0x0800;
const LOOPBACK_HEADER_LEN: usize = 4;

const HEADER_WORD_LEN: usize = 4;
const IPV4_MIN_HEADER_LEN: usize = 20;
const IPV4_VERSION: u8 = 4;
const IPV4_HEADER_WORDS_MASK: u8 = 0x0F;
const IPV4_TOTAL_LEN_OFFSET: usize = 2;
const IPV4_FRAGMENT_OFFSET: usize = 6;
const IPV4_FRAGMENT_OFFSET_MASK: u16 = 0x1FFF;
const IPV4_PROTOCOL_OFFSET: usize = 9;
const IPV4_SRC_OFFSET: usize = 12;
const IPV4_DST_OFFSET: usize = 16;
const IP_PROTOCOL_TCP: u8 = 6;

const TCP_MIN_HEADER_LEN: usize = 20;
const TCP_SRC_PORT_OFFSET: usize = 0;
const TCP_DST_PORT_OFFSET: usize = 2;
const TCP_SEQ_OFFSET: usize = 4;
const TCP_DATA_OFFSET_OFFSET: usize = 12;
const TCP_FLAGS_OFFSET: usize = 13;

#[derive(Debug, Clone)]
pub struct RawFrame {
    pub ts_us: i64,
    pub linktype: i32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub src: SocketAddrV4,
    pub dst: SocketAddrV4,
}

#[derive(Debug)]
pub struct TcpSegment<'a> {
    pub flow: FlowKey,
    pub seq: u32,
    pub syn: bool,
    pub fin: bool,
    pub rst: bool,
    pub payload: &'a [u8],
}

pub fn parse_tcp(linktype: i32, data: &[u8]) -> Option<TcpSegment<'_>> {
    parse_ipv4_tcp(ipv4_packet(linktype, data)?)
}

fn ipv4_packet(linktype: i32, data: &[u8]) -> Option<&[u8]> {
    match linktype {
        LINKTYPE_ETHERNET => {
            let mut ethertype = be_u16_at(data, ETHERTYPE_OFFSET)?;
            let mut header_len = ETHERNET_HEADER_LEN;
            if ethertype == ETHERTYPE_VLAN {
                ethertype = be_u16_at(data, VLAN_ETHERTYPE_OFFSET)?;
                header_len = VLAN_ETHERNET_HEADER_LEN;
            }
            if ethertype != ETHERTYPE_IPV4 {
                return None;
            }
            data.get(header_len..)
        }
        LINKTYPE_NULL | LINKTYPE_LOOP => data.get(LOOPBACK_HEADER_LEN..),
        LINKTYPE_RAW => Some(data),
        _ => None,
    }
}

fn be_u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *data.get(offset)?,
        *data.get(offset + 1)?,
    ]))
}

fn be_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([data[offset], data[offset + 1]])
}

fn ipv4_addr(data: &[u8], offset: usize) -> Ipv4Addr {
    Ipv4Addr::new(
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    )
}

fn parse_ipv4_tcp(ip: &[u8]) -> Option<TcpSegment<'_>> {
    if ip.len() < IPV4_MIN_HEADER_LEN
        || ip[0] >> 4 != IPV4_VERSION
        || ip[IPV4_PROTOCOL_OFFSET] != IP_PROTOCOL_TCP
    {
        return None;
    }
    let ip_header_len = usize::from(ip[0] & IPV4_HEADER_WORDS_MASK) * HEADER_WORD_LEN;
    let total_len = usize::from(be_u16(ip, IPV4_TOTAL_LEN_OFFSET));
    let packet_end = if total_len == 0 {
        ip.len()
    } else {
        total_len.min(ip.len())
    };
    if ip_header_len < IPV4_MIN_HEADER_LEN || packet_end < ip_header_len + TCP_MIN_HEADER_LEN {
        return None;
    }
    if be_u16(ip, IPV4_FRAGMENT_OFFSET) & IPV4_FRAGMENT_OFFSET_MASK != 0 {
        return None;
    }
    let tcp = &ip[ip_header_len..packet_end];
    let tcp_header_len = usize::from(tcp[TCP_DATA_OFFSET_OFFSET] >> 4) * HEADER_WORD_LEN;
    if tcp_header_len < TCP_MIN_HEADER_LEN || tcp_header_len > tcp.len() {
        return None;
    }
    let flags = tcp[TCP_FLAGS_OFFSET];
    Some(TcpSegment {
        flow: FlowKey {
            src: SocketAddrV4::new(
                ipv4_addr(ip, IPV4_SRC_OFFSET),
                be_u16(tcp, TCP_SRC_PORT_OFFSET),
            ),
            dst: SocketAddrV4::new(
                ipv4_addr(ip, IPV4_DST_OFFSET),
                be_u16(tcp, TCP_DST_PORT_OFFSET),
            ),
        },
        seq: u32::from_be_bytes([
            tcp[TCP_SEQ_OFFSET],
            tcp[TCP_SEQ_OFFSET + 1],
            tcp[TCP_SEQ_OFFSET + 2],
            tcp[TCP_SEQ_OFFSET + 3],
        ]),
        syn: flags & TCP_FLAG_SYN != 0,
        fin: flags & TCP_FLAG_FIN != 0,
        rst: flags & TCP_FLAG_RST != 0,
        payload: &tcp[tcp_header_len..],
    })
}

#[cfg(test)]
pub(crate) const TCP_FLAGS_PSH_ACK: u8 = 0x18;

#[cfg(test)]
pub(crate) fn build_ipv4_tcp(
    src: SocketAddrV4,
    dst: SocketAddrV4,
    seq: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let total_len = (IPV4_MIN_HEADER_LEN + TCP_MIN_HEADER_LEN + payload.len()) as u16;
    let mut packet = vec![0x45, 0, 0, 0, 0, 0, 0x40, 0, 64, IP_PROTOCOL_TCP, 0, 0];
    packet[IPV4_TOTAL_LEN_OFFSET..IPV4_TOTAL_LEN_OFFSET + 2]
        .copy_from_slice(&total_len.to_be_bytes());
    packet.extend_from_slice(&src.ip().octets());
    packet.extend_from_slice(&dst.ip().octets());
    packet.extend_from_slice(&src.port().to_be_bytes());
    packet.extend_from_slice(&dst.port().to_be_bytes());
    packet.extend_from_slice(&seq.to_be_bytes());
    packet.extend_from_slice(&0u32.to_be_bytes());
    packet.push(0x50);
    packet.push(flags);
    packet.extend_from_slice(&[0xFF, 0xFF, 0, 0, 0, 0]);
    packet.extend_from_slice(payload);
    packet
}

#[cfg(test)]
pub(crate) fn build_eth_tcp(
    src: SocketAddrV4,
    dst: SocketAddrV4,
    seq: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    const MIN_ETHERNET_FRAME_LEN: usize = 60;
    let mut frame = vec![0u8; ETHERTYPE_OFFSET];
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.extend(build_ipv4_tcp(src, dst, seq, flags, payload));
    frame.resize(frame.len().max(MIN_ETHERNET_FRAME_LEN), 0);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(ip: [u8; 4], port: u16) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::from(ip), port)
    }

    #[test]
    fn parses_ethernet_tcp_and_ignores_padding() {
        let frame = build_eth_tcp(
            addr([10, 0, 0, 1], 7777),
            addr([192, 168, 1, 5], 50000),
            42,
            TCP_FLAGS_PSH_ACK,
            b"hi",
        );
        let segment = parse_tcp(LINKTYPE_ETHERNET, &frame).unwrap();
        assert_eq!(segment.flow.src, addr([10, 0, 0, 1], 7777));
        assert_eq!(segment.flow.dst, addr([192, 168, 1, 5], 50000));
        assert_eq!(segment.seq, 42);
        assert_eq!(segment.payload, b"hi");
        assert!(!segment.syn && !segment.fin && !segment.rst);
    }

    #[test]
    fn parses_npcap_loopback() {
        let mut frame = vec![2, 0, 0, 0];
        frame.extend(build_ipv4_tcp(
            addr([127, 0, 0, 1], 1),
            addr([127, 0, 0, 1], 2),
            7,
            TCP_FLAG_SYN,
            b"",
        ));
        let segment = parse_tcp(LINKTYPE_NULL, &frame).unwrap();
        assert!(segment.syn);
        assert!(segment.payload.is_empty());
    }

    #[test]
    fn rejects_udp() {
        const IP_PROTOCOL_UDP: u8 = 17;
        let mut frame = build_eth_tcp(addr([1, 1, 1, 1], 1), addr([2, 2, 2, 2], 2), 0, 0, b"x");
        frame[ETHERNET_HEADER_LEN + IPV4_PROTOCOL_OFFSET] = IP_PROTOCOL_UDP;
        assert!(parse_tcp(LINKTYPE_ETHERNET, &frame).is_none());
    }
}
