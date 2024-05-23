#![forbid(unsafe_code)]

use byteorder::ReadBytesExt;
use std::io::{self, BufRead};

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitSequence {
    bits: u16,
    len: u8,
}

impl BitSequence {
    pub fn new(bits: u16, len: u8) -> Self {
        Self { bits, len }
    }

    pub fn bits(&self) -> u16 {
        self.bits
    }

    pub fn len(&self) -> u8 {
        self.len
    }

    pub fn concat(self, other: Self) -> Self {
        Self {
            bits: (self.bits << other.len) + other.bits,
            len: self.len + other.len,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

pub struct BitReader<T> {
    stream: T,
    bit_sequence: BitSequence,
}

impl<T: BufRead> BitReader<T> {
    pub fn new(stream: T) -> Self {
        Self {
            stream,
            bit_sequence: BitSequence::new(0, 0),
        }
    }

    pub fn read_bits(&mut self, len: u8) -> io::Result<BitSequence> {
        let mut already_len: u8 = self.bit_sequence.len();
        let mut bit_sequence: u32 = self.bit_sequence.bits().into();
        while already_len < len {
            let new_bits: u32 = self.stream.read_u8()?.into();
            bit_sequence += new_bits << already_len;
            already_len += 8;
        }
        let ans: u16 = (bit_sequence & ((1 << len) - 1)) as u16;
        self.bit_sequence = BitSequence::new((bit_sequence >> len) as u16, already_len - len);
        Ok(BitSequence::new(ans, len))
    }

    pub fn borrow_reader_from_boundary(&mut self) -> &mut T {
        self.bit_sequence.len = 0;
        self.bit_sequence.bits = 0;
        &mut self.stream
    }
}

////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::ReadBytesExt;

    #[test]
    fn read_bits() -> io::Result<()> {
        let data: &[u8] = &[0b01100011, 0b11011011, 0b10101111];
        let mut reader = BitReader::new(data);
        assert_eq!(reader.read_bits(1)?, BitSequence::new(0b1, 1));
        assert_eq!(reader.read_bits(2)?, BitSequence::new(0b01, 2));
        assert_eq!(reader.read_bits(3)?, BitSequence::new(0b100, 3));
        assert_eq!(reader.read_bits(4)?, BitSequence::new(0b1101, 4));
        assert_eq!(reader.read_bits(5)?, BitSequence::new(0b10110, 5));
        assert_eq!(reader.read_bits(8)?, BitSequence::new(0b01011111, 8));
        assert_eq!(
            reader.read_bits(2).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        Ok(())
    }

    #[test]
    fn borrow_reader_from_boundary() -> io::Result<()> {
        let data: &[u8] = &[0b01100011, 0b11011011, 0b10101111];
        let mut reader = BitReader::new(data);
        assert_eq!(reader.read_bits(3)?, BitSequence::new(0b011, 3));
        assert_eq!(reader.borrow_reader_from_boundary().read_u8()?, 0b11011011);
        assert_eq!(reader.read_bits(8)?, BitSequence::new(0b10101111, 8));
        Ok(())
    }
}
