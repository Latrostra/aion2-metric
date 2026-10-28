use std::collections::BTreeMap;

pub const MAX_PENDING_BYTES: usize = 512 * 1024;
const SYN_SEQ_LEN: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    Data(Vec<u8>),
    Gap,
}

#[derive(Default)]
pub struct Reassembler {
    started: bool,
    pub aligned: bool,
    next_seq: u32,
    next_offset: u64,
    pending: BTreeMap<u64, Vec<u8>>,
    pending_bytes: usize,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, seq: u32, syn: bool, payload: &[u8]) -> Vec<Chunk> {
        let mut out = Vec::new();
        let data_seq = if syn {
            seq.wrapping_add(SYN_SEQ_LEN)
        } else {
            seq
        };
        if !self.started {
            if !syn && payload.is_empty() {
                return out;
            }
            self.started = true;
            self.aligned = syn;
            self.next_seq = data_seq;
        }
        if payload.is_empty() {
            return out;
        }
        let seq_delta = i64::from(data_seq.wrapping_sub(self.next_seq) as i32);
        let next_offset = self.next_offset as i64;
        let start = next_offset + seq_delta;
        let end = start + payload.len() as i64;
        if end <= next_offset {
            return out;
        }
        let (start, payload) = if start < next_offset {
            (self.next_offset, &payload[(next_offset - start) as usize..])
        } else {
            (start as u64, payload)
        };
        if start == self.next_offset {
            self.deliver(payload, &mut out);
            self.drain_pending(&mut out);
        } else {
            self.buffer_ahead(start, payload, &mut out);
        }
        out
    }

    pub fn flush_gap(&mut self) -> Vec<Chunk> {
        let mut out = Vec::new();
        self.skip_gap(&mut out);
        out
    }

    fn buffer_ahead(&mut self, start: u64, payload: &[u8], out: &mut Vec<Chunk>) {
        let slot = self.pending.entry(start).or_default();
        if payload.len() > slot.len() {
            self.pending_bytes += payload.len() - slot.len();
            *slot = payload.to_vec();
        }
        if self.pending_bytes > MAX_PENDING_BYTES {
            self.skip_gap(out);
        }
    }

    fn deliver(&mut self, data: &[u8], out: &mut Vec<Chunk>) {
        self.next_offset += data.len() as u64;
        self.next_seq = self.next_seq.wrapping_add(data.len() as u32);
        out.push(Chunk::Data(data.to_vec()));
    }

    fn drain_pending(&mut self, out: &mut Vec<Chunk>) {
        while let Some((&offset, _)) = self.pending.first_key_value() {
            if offset > self.next_offset {
                break;
            }
            let data = self.pending.remove(&offset).expect("key exists");
            self.pending_bytes -= data.len();
            let end = offset + data.len() as u64;
            if end > self.next_offset {
                let already_delivered = (self.next_offset - offset) as usize;
                self.deliver(&data[already_delivered..], out);
            }
        }
    }

    fn skip_gap(&mut self, out: &mut Vec<Chunk>) {
        let Some((&offset, _)) = self.pending.first_key_value() else {
            return;
        };
        let skipped = offset - self.next_offset;
        self.next_seq = self.next_seq.wrapping_add(skipped as u32);
        self.next_offset = offset;
        out.push(Chunk::Gap);
        self.drain_pending(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(chunks: &[Chunk]) -> Vec<Vec<u8>> {
        chunks
            .iter()
            .map(|chunk| match chunk {
                Chunk::Data(bytes) => bytes.clone(),
                Chunk::Gap => b"<GAP>".to_vec(),
            })
            .collect()
    }

    #[test]
    fn in_order() {
        let mut reassembler = Reassembler::new();
        assert_eq!(
            data(&reassembler.push(1000, false, b"abc")),
            vec![b"abc".to_vec()]
        );
        assert_eq!(
            data(&reassembler.push(1003, false, b"def")),
            vec![b"def".to_vec()]
        );
    }

    #[test]
    fn out_of_order_is_buffered() {
        let mut reassembler = Reassembler::new();
        reassembler.push(1000, false, b"abc");
        assert!(reassembler.push(1006, false, b"ghi").is_empty());
        assert_eq!(
            data(&reassembler.push(1003, false, b"def")),
            vec![b"def".to_vec(), b"ghi".to_vec()]
        );
    }

    #[test]
    fn retransmit_overlap_is_trimmed() {
        let mut reassembler = Reassembler::new();
        reassembler.push(1000, false, b"abcd");
        assert!(reassembler.push(1000, false, b"ab").is_empty());
        assert_eq!(
            data(&reassembler.push(1002, false, b"cdef")),
            vec![b"ef".to_vec()]
        );
    }

    #[test]
    fn syn_marks_aligned_and_consumes_one_seq() {
        let mut reassembler = Reassembler::new();
        assert!(reassembler.push(999, true, b"").is_empty());
        assert!(reassembler.aligned);
        assert_eq!(
            data(&reassembler.push(1000, false, b"x")),
            vec![b"x".to_vec()]
        );
    }

    #[test]
    fn sequence_wraparound() {
        let mut reassembler = Reassembler::new();
        reassembler.push(u32::MAX - 1, false, b"ab");
        assert_eq!(
            data(&reassembler.push(0, false, b"cd")),
            vec![b"cd".to_vec()]
        );
    }

    #[test]
    fn flush_gap_skips_hole() {
        let mut reassembler = Reassembler::new();
        reassembler.push(1000, false, b"abc");
        assert!(reassembler.push(1010, false, b"zz").is_empty());
        assert_eq!(
            data(&reassembler.flush_gap()),
            vec![b"<GAP>".to_vec(), b"zz".to_vec()]
        );
    }

    #[test]
    fn pending_overflow_forces_gap() {
        let mut reassembler = Reassembler::new();
        reassembler.push(1000, false, b"a");
        let oversized = vec![7u8; MAX_PENDING_BYTES + 1];
        let out = reassembler.push(1002, false, &oversized);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Chunk::Gap);
    }
}
