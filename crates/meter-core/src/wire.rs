pub const MAX_VARINT_BYTES: usize = 5;
pub const VARINT_VALUE_MASK: u8 = 0x7F;
pub const VARINT_CONTINUATION_BIT: u8 = 0x80;
pub const VARINT_BITS_PER_BYTE: usize = 7;

pub fn read_varint(data: &[u8]) -> Option<(u32, usize)> {
    let mut value: u32 = 0;
    for (index, &byte) in data.iter().take(MAX_VARINT_BYTES).enumerate() {
        value |= u32::from(byte & VARINT_VALUE_MASK) << (VARINT_BITS_PER_BYTE * index);
        if byte & VARINT_CONTINUATION_BIT == 0 {
            return Some((value, index + 1));
        }
    }
    None
}

pub fn u16_le(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

pub fn u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self::at(data, 0)
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self {
            data,
            pos: pos.min(data.len()),
        }
    }

    pub fn varint(&mut self) -> Option<u32> {
        let (value, len) = read_varint(&self.data[self.pos..])?;
        self.pos += len;
        Some(value)
    }

    pub fn u8(&mut self) -> Option<u8> {
        let value = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(value)
    }

    pub fn u32(&mut self) -> Option<u32> {
        let value = u32_le(self.data, self.pos)?;
        self.pos += 4;
        Some(value)
    }

    pub fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.pos..self.pos + len)?;
        self.pos += len;
        Some(bytes)
    }

    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }
}

pub fn hex(data: &[u8], max_bytes: usize) -> String {
    let mut text = data
        .iter()
        .take(max_bytes)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if data.len() > max_bytes {
        text.push_str(" …");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_single_byte() {
        assert_eq!(read_varint(&[0x05, 0xFF]), Some((5, 1)));
    }

    #[test]
    fn varint_two_bytes() {
        assert_eq!(read_varint(&[0xAC, 0x02]), Some((300, 2)));
    }

    #[test]
    fn varint_incomplete() {
        assert_eq!(read_varint(&[0x80]), None);
        assert_eq!(read_varint(&[]), None);
    }

    #[test]
    fn varint_too_long() {
        assert_eq!(read_varint(&[0x80; MAX_VARINT_BYTES + 1]), None);
    }

    #[test]
    fn u32_le_reads_and_checks_bounds() {
        assert_eq!(u32_le(&[1, 0, 0, 0, 9], 0), Some(1));
        assert_eq!(u32_le(&[1, 0, 0], 0), None);
    }

    #[test]
    fn hex_truncates() {
        assert_eq!(hex(&[0xAB, 0x01, 0x02], 2), "ab 01 …");
    }
}
