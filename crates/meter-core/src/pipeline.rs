use std::collections::{HashMap, HashSet};

use crate::frame::FrameDecoder;
use crate::net::{FlowKey, RawFrame, parse_tcp};
use crate::tcp::{Chunk, Reassembler};

const MICROS_PER_MILLI: i64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePacket {
    pub ts_us: i64,
    pub flow: FlowKey,
    pub opcode: u16,
    pub body: Vec<u8>,
}

impl GamePacket {
    pub fn ts_ms(&self) -> i64 {
        self.ts_us / MICROS_PER_MILLI
    }
}

struct Stream {
    tcp: Reassembler,
    frames: FrameDecoder,
}

#[derive(Default)]
pub struct Pipeline {
    streams: HashMap<FlowKey, Stream>,
    allowed: Option<HashSet<FlowKey>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_allowed(&mut self, allowed: Option<HashSet<FlowKey>>) {
        if let Some(allowed_flows) = &allowed {
            self.streams.retain(|flow, _| allowed_flows.contains(flow));
        }
        self.allowed = allowed;
    }

    pub fn push(&mut self, frame: &RawFrame, out: &mut Vec<GamePacket>) {
        let Some(segment) = parse_tcp(frame.linktype, &frame.data) else {
            return;
        };
        if self
            .allowed
            .as_ref()
            .is_some_and(|allowed_flows| !allowed_flows.contains(&segment.flow))
        {
            return;
        }
        let flow = segment.flow;
        let ts_us = frame.ts_us;
        let stream = self.streams.entry(flow).or_insert_with(|| Stream {
            tcp: Reassembler::new(),
            frames: FrameDecoder::new(segment.syn),
        });
        for chunk in stream.tcp.push(segment.seq, segment.syn, segment.payload) {
            match chunk {
                Chunk::Gap => stream.frames.reset(),
                Chunk::Data(data) => stream.frames.feed(&data, &mut |opcode, body| {
                    out.push(GamePacket {
                        ts_us,
                        flow,
                        opcode,
                        body: body.to_vec(),
                    })
                }),
            }
        }
        if segment.fin || segment.rst {
            self.streams.remove(&flow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::test_util::{heartbeat, pkt};
    use crate::net::{LINKTYPE_ETHERNET, TCP_FLAG_SYN, TCP_FLAGS_PSH_ACK, build_eth_tcp};
    use crate::opcodes;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn frames() -> Vec<RawFrame> {
        let server = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 7777);
        let client = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 5), 50000);
        let mut stream = heartbeat();
        stream.extend(pkt(opcodes::DAMAGE.to_be_bytes(), b"hit!"));
        let (first_part, second_part) = stream.split_at(5);
        let first_data_seq = 101;
        [
            build_eth_tcp(server, client, 100, TCP_FLAG_SYN, &[]),
            build_eth_tcp(
                server,
                client,
                first_data_seq,
                TCP_FLAGS_PSH_ACK,
                first_part,
            ),
            build_eth_tcp(
                server,
                client,
                first_data_seq + first_part.len() as u32,
                TCP_FLAGS_PSH_ACK,
                second_part,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, data)| RawFrame {
            ts_us: index as i64,
            linktype: LINKTYPE_ETHERNET,
            data,
        })
        .collect()
    }

    #[test]
    fn syn_then_split_segments_yield_packets() {
        let mut pipeline = Pipeline::new();
        let mut out = Vec::new();
        for frame in frames() {
            pipeline.push(&frame, &mut out);
        }
        let opcodes: Vec<u16> = out.iter().map(|packet| packet.opcode).collect();
        assert_eq!(opcodes, vec![opcodes::HEARTBEAT, opcodes::DAMAGE]);
        assert_eq!(out[1].body, b"hit!");
        assert_eq!(out[1].ts_us, 2);
    }

    #[test]
    fn allowed_set_filters_flows() {
        let mut pipeline = Pipeline::new();
        pipeline.set_allowed(Some(HashSet::new()));
        let mut out = Vec::new();
        for frame in frames() {
            pipeline.push(&frame, &mut out);
        }
        assert!(out.is_empty());
    }
}
