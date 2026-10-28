use crate::opcodes::OPCODE_LEN;
use crate::wire::{MAX_VARINT_BYTES, read_varint, u32_le};

pub const MAX_FRAME: usize = 65_535;
pub const MAX_UNBUNDLED_WAIT: usize = 16_384;
pub const MAX_BUNDLE: usize = 8 * 1024 * 1024;
pub const BUNDLE_MARKER: [u8; 2] = [0xFF, 0xFF];
pub const BUNDLE_HEADER_LEN: usize = BUNDLE_MARKER.len() + BUNDLE_SIZE_LEN;
const BUNDLE_SIZE_LEN: usize = 4;
const LENGTH_FIELD_BIAS: usize = 4;
const MIN_DECLARED_LEN: usize = 6;
const PADDING_BYTE: u8 = 0x00;
const FLAG_BYTES: std::ops::RangeInclusive<u8> = 0xF0..=0xFE;
const MAX_HUNT_BYTES: usize = 256 * 1024;
const MAX_BUNDLE_DEPTH: u8 = 4;
const INITIAL_BUFFER_CAPACITY: usize = 128 * 1024;
const HEARTBEAT_SIGNATURE_LEN: usize = 3;
const HEARTBEAT_SIGNATURES: [[u8; HEARTBEAT_SIGNATURE_LEN]; 2] =
    [[0x0E, 0x00, 0x36], [0x06, 0x00, 0x36]];

pub fn frame_size(declared_len: usize, varint_len: usize) -> usize {
    declared_len + varint_len - LENGTH_FIELD_BIAS
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    pub frames: u64,
    pub bundles: u64,
    pub bad_bundles: u64,
    pub resyncs: u64,
    pub dropped_bytes: u64,
}

pub struct FrameDecoder {
    buffer: Vec<u8>,
    hunting: bool,
    hunted_bytes: usize,
    pub stats: FrameStats,
}

enum NextFrame {
    Complete { size: usize, varint_len: usize },
    Incomplete,
    Corrupt,
}

impl FrameDecoder {
    pub fn new(starts_aligned: bool) -> Self {
        Self {
            buffer: Vec::with_capacity(INITIAL_BUFFER_CAPACITY),
            hunting: !starts_aligned,
            hunted_bytes: 0,
            stats: FrameStats::default(),
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.hunting = true;
        self.hunted_bytes = 0;
    }

    pub fn feed(&mut self, data: &[u8], on_packet: &mut dyn FnMut(u16, &[u8])) {
        if data.is_empty() {
            return;
        }
        let mut buffer = std::mem::take(&mut self.buffer);
        buffer.extend_from_slice(data);
        let mut pos = 0;
        while pos < buffer.len() {
            let span = &buffer[pos..];
            if self.hunting {
                match find_heartbeat(span) {
                    Some(offset) => {
                        pos += self.skip_to_heartbeat(offset);
                        continue;
                    }
                    None => {
                        pos += self.skip_hunted_bytes(span.len());
                        break;
                    }
                }
            }
            if span[0] == PADDING_BYTE {
                pos += 1;
                continue;
            }
            match next_frame(span) {
                NextFrame::Complete { size, varint_len } => {
                    handle_frame(&mut self.stats, &span[..size], varint_len, 0, on_packet);
                    pos += size;
                }
                NextFrame::Incomplete => break,
                NextFrame::Corrupt => pos += self.resync(),
            }
        }
        buffer.drain(..pos);
        self.buffer = buffer;
    }

    fn skip_to_heartbeat(&mut self, offset: usize) -> usize {
        self.stats.dropped_bytes += offset as u64;
        self.hunting = false;
        offset
    }

    fn skip_hunted_bytes(&mut self, span_len: usize) -> usize {
        let kept_for_split_signature = span_len.min(HEARTBEAT_SIGNATURE_LEN - 1);
        let skipped = span_len - kept_for_split_signature;
        self.stats.dropped_bytes += skipped as u64;
        self.hunted_bytes += skipped;
        if self.hunted_bytes > MAX_HUNT_BYTES {
            self.hunting = false;
        }
        skipped
    }

    fn resync(&mut self) -> usize {
        const SKIPPED_BYTES: usize = 1;
        self.stats.resyncs += 1;
        self.stats.dropped_bytes += SKIPPED_BYTES as u64;
        if self.hunted_bytes <= MAX_HUNT_BYTES {
            self.hunting = true;
            self.hunted_bytes = 0;
        }
        SKIPPED_BYTES
    }
}

fn next_frame(span: &[u8]) -> NextFrame {
    let Some((declared_len, varint_len)) = read_varint(span) else {
        return if span.len() < MAX_VARINT_BYTES {
            NextFrame::Incomplete
        } else {
            NextFrame::Corrupt
        };
    };
    let declared_len = declared_len as usize;
    if declared_len < MIN_DECLARED_LEN {
        return NextFrame::Corrupt;
    }
    let size = frame_size(declared_len, varint_len);
    if size > MAX_FRAME {
        return NextFrame::Corrupt;
    }
    if size > span.len() {
        if size > MAX_UNBUNDLED_WAIT && !is_bundle(span, varint_len) {
            return NextFrame::Corrupt;
        }
        return NextFrame::Incomplete;
    }
    NextFrame::Complete { size, varint_len }
}

fn find_heartbeat(span: &[u8]) -> Option<usize> {
    span.windows(HEARTBEAT_SIGNATURE_LEN).position(|window| {
        HEARTBEAT_SIGNATURES
            .iter()
            .any(|signature| window == signature)
    })
}

fn payload_start(frame: &[u8], varint_len: usize) -> usize {
    if frame
        .get(varint_len)
        .is_some_and(|byte| FLAG_BYTES.contains(byte))
    {
        varint_len + 1
    } else {
        varint_len
    }
}

fn is_bundle(frame: &[u8], varint_len: usize) -> bool {
    let start = payload_start(frame, varint_len);
    frame.get(start..start + BUNDLE_MARKER.len()) == Some(&BUNDLE_MARKER[..])
}

fn handle_frame(
    stats: &mut FrameStats,
    frame: &[u8],
    varint_len: usize,
    depth: u8,
    on_packet: &mut dyn FnMut(u16, &[u8]),
) {
    stats.frames += 1;
    let payload = &frame[payload_start(frame, varint_len)..];
    if payload.len() < OPCODE_LEN {
        return;
    }
    if payload.starts_with(&BUNDLE_MARKER) {
        stats.bundles += 1;
        decode_bundle(stats, payload, depth, on_packet);
        return;
    }
    let opcode = u16::from_be_bytes([payload[0], payload[1]]);
    on_packet(opcode, &payload[OPCODE_LEN..]);
}

fn decode_bundle(
    stats: &mut FrameStats,
    payload: &[u8],
    depth: u8,
    on_packet: &mut dyn FnMut(u16, &[u8]),
) {
    let decompressed_len = u32_le(payload, BUNDLE_MARKER.len())
        .map(|len| len as usize)
        .filter(|&len| len > 0 && len <= MAX_BUNDLE && depth <= MAX_BUNDLE_DEPTH);
    let Some(decompressed_len) = decompressed_len else {
        stats.bad_bundles += 1;
        return;
    };
    let mut decompressed = vec![0u8; decompressed_len];
    match lz4_flex::block::decompress_into(&payload[BUNDLE_HEADER_LEN..], &mut decompressed) {
        Ok(written) if written > 0 => {
            walk_bundle(stats, &decompressed[..written], depth + 1, on_packet)
        }
        _ => stats.bad_bundles += 1,
    }
}

fn walk_bundle(
    stats: &mut FrameStats,
    data: &[u8],
    depth: u8,
    on_packet: &mut dyn FnMut(u16, &[u8]),
) {
    let mut pos = 0;
    while pos < data.len() {
        if data[pos] == PADDING_BYTE {
            pos += 1;
            continue;
        }
        let span = &data[pos..];
        let Some((declared_len, varint_len)) = read_varint(span) else {
            return;
        };
        let declared_len = declared_len as usize;
        if declared_len < MIN_DECLARED_LEN {
            return;
        }
        let size = frame_size(declared_len, varint_len);
        if size > span.len() {
            return;
        }
        handle_frame(stats, &span[..size], varint_len, depth, on_packet);
        pos += size;
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::LENGTH_FIELD_BIAS;
    use crate::opcodes;
    use crate::wire::{VARINT_BITS_PER_BYTE, VARINT_CONTINUATION_BIT, VARINT_VALUE_MASK};

    pub fn wrap(payload: &[u8]) -> Vec<u8> {
        let mut remaining = (payload.len() + LENGTH_FIELD_BIAS) as u32;
        let mut frame = Vec::new();
        loop {
            let low_bits = (remaining & u32::from(VARINT_VALUE_MASK)) as u8;
            remaining >>= VARINT_BITS_PER_BYTE;
            if remaining == 0 {
                frame.push(low_bits);
                break;
            }
            frame.push(low_bits | VARINT_CONTINUATION_BIT);
        }
        frame.extend_from_slice(payload);
        frame
    }

    pub fn pkt(opcode: [u8; 2], body: &[u8]) -> Vec<u8> {
        let mut payload = opcode.to_vec();
        payload.extend_from_slice(body);
        wrap(&payload)
    }

    pub fn heartbeat() -> Vec<u8> {
        pkt(opcodes::HEARTBEAT.to_be_bytes(), &[0; 8])
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;
    use crate::opcodes::{CAST, DAMAGE, DOT, HEARTBEAT};

    fn decode(decoder: &mut FrameDecoder, data: &[u8]) -> Vec<(u16, Vec<u8>)> {
        let mut got = Vec::new();
        decoder.feed(data, &mut |opcode, body| got.push((opcode, body.to_vec())));
        got
    }

    fn opcodes_of(packets: Vec<(u16, Vec<u8>)>) -> Vec<u16> {
        packets.into_iter().map(|(opcode, _)| opcode).collect()
    }

    fn bundle_payload(inner: &[u8]) -> Vec<u8> {
        let mut payload = BUNDLE_MARKER.to_vec();
        payload.extend_from_slice(&(inner.len() as u32).to_le_bytes());
        payload.extend(lz4_flex::block::compress(inner));
        payload
    }

    #[test]
    fn heartbeat_helper_matches_wire_signature() {
        assert_eq!(
            &heartbeat()[..HEARTBEAT_SIGNATURE_LEN],
            &HEARTBEAT_SIGNATURES[0]
        );
    }

    #[test]
    fn single_packet() {
        let mut decoder = FrameDecoder::new(true);
        let got = decode(&mut decoder, &pkt(DAMAGE.to_be_bytes(), b"dmg"));
        assert_eq!(got, vec![(DAMAGE, b"dmg".to_vec())]);
    }

    #[test]
    fn flag_byte_before_opcode_is_skipped() {
        let mut decoder = FrameDecoder::new(true);
        let mut payload = vec![0xF3];
        payload.extend_from_slice(&DAMAGE.to_be_bytes());
        payload.push(0xAA);
        let got = decode(&mut decoder, &wrap(&payload));
        assert_eq!(got, vec![(DAMAGE, vec![0xAA])]);
    }

    #[test]
    fn byte_by_byte_feed() {
        let mut decoder = FrameDecoder::new(true);
        let mut got = Vec::new();
        for byte in pkt(DAMAGE.to_be_bytes(), b"split") {
            got.extend(decode(&mut decoder, &[byte]));
        }
        assert_eq!(got, vec![(DAMAGE, b"split".to_vec())]);
    }

    #[test]
    fn two_frames_with_padding() {
        let mut decoder = FrameDecoder::new(true);
        let mut data = pkt(CAST.to_be_bytes(), b"a");
        data.extend_from_slice(&[PADDING_BYTE; 3]);
        data.extend(pkt(DAMAGE.to_be_bytes(), b"b"));
        assert_eq!(opcodes_of(decode(&mut decoder, &data)), vec![CAST, DAMAGE]);
    }

    #[test]
    fn lz4_bundle_yields_inner_packets() {
        let mut inner = pkt(DAMAGE.to_be_bytes(), b"dmg1");
        inner.push(PADDING_BYTE);
        inner.extend(pkt(DOT.to_be_bytes(), b"dot1"));
        let mut decoder = FrameDecoder::new(true);
        let got = decode(&mut decoder, &wrap(&bundle_payload(&inner)));
        assert_eq!(
            got,
            vec![(DAMAGE, b"dmg1".to_vec()), (DOT, b"dot1".to_vec())]
        );
        assert_eq!(decoder.stats.bundles, 1);
    }

    #[test]
    fn bad_bundle_is_counted_not_emitted() {
        let mut payload = BUNDLE_MARKER.to_vec();
        payload.extend_from_slice(&100u32.to_le_bytes());
        payload.push(0xF0);
        let mut decoder = FrameDecoder::new(true);
        assert!(decode(&mut decoder, &wrap(&payload)).is_empty());
        assert_eq!(decoder.stats.bad_bundles, 1);
    }

    #[test]
    fn unaligned_start_hunts_for_heartbeat() {
        let mut data = vec![0x41; 7];
        data.extend(heartbeat());
        data.extend(pkt(DAMAGE.to_be_bytes(), b"x"));
        let mut decoder = FrameDecoder::new(false);
        assert_eq!(
            opcodes_of(decode(&mut decoder, &data)),
            vec![HEARTBEAT, DAMAGE]
        );
        assert_eq!(decoder.stats.dropped_bytes, 7);
    }

    #[test]
    fn reset_drops_partial_frame_and_hunts() {
        let mut decoder = FrameDecoder::new(true);
        let frame = pkt(DAMAGE.to_be_bytes(), b"lost");
        assert!(decode(&mut decoder, &frame[..3]).is_empty());
        decoder.reset();
        let mut data = frame[3..].to_vec();
        data.extend(heartbeat());
        assert_eq!(opcodes_of(decode(&mut decoder, &data)), vec![HEARTBEAT]);
    }
}
