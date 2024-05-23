#![forbid(unsafe_code)]

use std::{collections::HashMap, convert::TryFrom, io::BufRead};

use anyhow::{anyhow, bail, Result};

use crate::bit_reader::{BitReader, BitSequence};

////////////////////////////////////////////////////////////////////////////////

const SPECIAL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

pub fn decode_fixed_trees() -> Result<(HuffmanCoding<LitLenToken>, HuffmanCoding<DistanceToken>)> {
    let distancetoken = [5u8; 32];
    let mut letlentoken = vec![];
    letlentoken.extend([8u8; 144]);
    letlentoken.extend([9u8; 112]);
    letlentoken.extend([7u8; 24]);
    letlentoken.extend([8u8; 8]);
    Ok((
        HuffmanCoding::from_lengths(&letlentoken)?,
        HuffmanCoding::from_lengths(&distancetoken)?,
    ))
}

pub fn decode_codelen_token<T: BufRead>(
    bit_reader: &mut BitReader<T>,
    hclen: u16,
) -> Result<HuffmanCoding<TreeCodeToken>> {
    let mut cl: Vec<u8> = vec![0; 19];
    for pos in &SPECIAL_ORDER[..(hclen + 4).into()] {
        cl[*pos] = bit_reader.read_bits(3)?.bits() as u8;
    }
    HuffmanCoding::from_lengths(&cl)
}

pub fn decode_letlen_token<T: BufRead>(
    bit_reader: &mut BitReader<T>,
    hlit: u16,
    cl_huffman: &HuffmanCoding<TreeCodeToken>,
) -> Result<HuffmanCoding<LitLenToken>> {
    let mut letlentoken: Vec<u8> = vec![0; 286];
    let mut pos: usize = 0;
    while pos < (hlit + 257).into() {
        let token = cl_huffman.read_symbol(bit_reader)?;
        match token {
            TreeCodeToken::Length(len) => {
                letlentoken[pos] = len;
                pos += 1;
            }
            TreeCodeToken::CopyPrev => {
                for i in 0..(3 + bit_reader.read_bits(2)?.bits()).into() {
                    letlentoken[pos] = letlentoken[pos - i - 1];
                    pos += 1;
                }
            }
            TreeCodeToken::RepeatZero { base, extra_bits } => {
                for _i in 0..(bit_reader.read_bits(extra_bits)?.bits() + base).into() {
                    letlentoken[pos] = 0;
                    pos += 1;
                }
            }
        }
    }
    HuffmanCoding::from_lengths(&letlentoken)
}

pub fn decode_distance_token<T: BufRead>(
    bit_reader: &mut BitReader<T>,
    hdist: u16,
    cl_huffman: &HuffmanCoding<TreeCodeToken>,
) -> Result<HuffmanCoding<DistanceToken>> {
    let mut distancetoken: Vec<u8> = vec![0; 32];
    let mut pos: usize = 0;
    while pos < (hdist + 1).into() {
        let token = cl_huffman.read_symbol(bit_reader)?;
        match token {
            TreeCodeToken::Length(len) => {
                distancetoken[pos] = len;
                pos += 1;
            }
            TreeCodeToken::CopyPrev => {
                for i in 0..(3 + bit_reader.read_bits(2)?.bits()).into() {
                    distancetoken[pos] = distancetoken[pos - i - 1];
                    pos += 1;
                }
            }
            TreeCodeToken::RepeatZero { base, extra_bits } => {
                for _i in 0..(bit_reader.read_bits(extra_bits)?.bits() + base).into() {
                    distancetoken[pos] = 0;
                    pos += 1;
                }
            }
        }
    }
    HuffmanCoding::from_lengths(&distancetoken)
}

pub fn decode_dynamic_tree<T: BufRead>(
    bit_reader: &mut BitReader<T>,
) -> Result<(HuffmanCoding<LitLenToken>, HuffmanCoding<DistanceToken>)> {
    let hlit = bit_reader.read_bits(5)?.bits();
    let hdist = bit_reader.read_bits(5)?.bits();
    let hclen = bit_reader.read_bits(4)?.bits();

    let cl_huffman = decode_codelen_token(bit_reader, hclen)?;
    let letlentoken = decode_letlen_token(bit_reader, hlit, &cl_huffman)?;
    let distancetoken = decode_distance_token(bit_reader, hdist, &cl_huffman)?;

    Ok((letlentoken, distancetoken))
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum TreeCodeToken {
    Length(u8),
    CopyPrev,
    RepeatZero { base: u16, extra_bits: u8 },
}

impl TryFrom<HuffmanCodeWord> for TreeCodeToken {
    type Error = anyhow::Error;

    fn try_from(value: HuffmanCodeWord) -> Result<Self> {
        match value.0 {
            0..=15 => Ok(TreeCodeToken::Length(value.0.try_into().unwrap())),
            16 => Ok(TreeCodeToken::CopyPrev),
            17 => Ok(TreeCodeToken::RepeatZero {
                base: 3,
                extra_bits: 3,
            }),
            18 => Ok(TreeCodeToken::RepeatZero {
                base: 11,
                extra_bits: 7,
            }),
            _ => Err(anyhow!("try from TreeCodeToken error")),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum LitLenToken {
    Literal(u8),
    EndOfBlock,
    Length { base: u16, extra_bits: u8 },
}

impl TryFrom<HuffmanCodeWord> for LitLenToken {
    type Error = anyhow::Error;

    fn try_from(value: HuffmanCodeWord) -> Result<Self> {
        match value.0 {
            0..=255 => Ok(LitLenToken::Literal(value.0.try_into().unwrap())),
            256 => Ok(LitLenToken::EndOfBlock),
            257..=264 => Ok(LitLenToken::Length {
                base: value.0 - 254,
                extra_bits: 0,
            }),
            265..=268 => Ok(LitLenToken::Length {
                base: 11 + 2 * (value.0 - 265),
                extra_bits: 1,
            }),
            269..=272 => Ok(LitLenToken::Length {
                base: 19 + 4 * (value.0 - 269),
                extra_bits: 2,
            }),
            273..=276 => Ok(LitLenToken::Length {
                base: 35 + 8 * (value.0 - 273),
                extra_bits: 3,
            }),
            277..=280 => Ok(LitLenToken::Length {
                base: 67 + 16 * (value.0 - 277),
                extra_bits: 4,
            }),
            281..=284 => Ok(LitLenToken::Length {
                base: 131 + 32 * (value.0 - 281),
                extra_bits: 5,
            }),
            285 => Ok(LitLenToken::Length {
                base: 258,
                extra_bits: 0,
            }),
            _ => Err(anyhow!("try from LitLenToken error")),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub struct DistanceToken {
    pub base: u16,
    pub extra_bits: u8,
}

impl TryFrom<HuffmanCodeWord> for DistanceToken {
    type Error = anyhow::Error;

    fn try_from(value: HuffmanCodeWord) -> Result<Self> {
        match value.0 {
            0..=3 => Ok(DistanceToken {
                base: value.0 + 1,
                extra_bits: 0,
            }),
            4..=5 => Ok(DistanceToken {
                base: 5 + 2 * (value.0 - 4),
                extra_bits: 1,
            }),
            6..=7 => Ok(DistanceToken {
                base: 9 + 4 * (value.0 - 6),
                extra_bits: 2,
            }),
            8..=9 => Ok(DistanceToken {
                base: 17 + 8 * (value.0 - 8),
                extra_bits: 3,
            }),
            10..=11 => Ok(DistanceToken {
                base: 33 + 16 * (value.0 - 10),
                extra_bits: 4,
            }),
            12..=13 => Ok(DistanceToken {
                base: 65 + 32 * (value.0 - 12),
                extra_bits: 5,
            }),
            14..=15 => Ok(DistanceToken {
                base: 129 + 64 * (value.0 - 14),
                extra_bits: 6,
            }),
            16..=17 => Ok(DistanceToken {
                base: 257 + 128 * (value.0 - 16),
                extra_bits: 7,
            }),
            18..=19 => Ok(DistanceToken {
                base: 513 + 256 * (value.0 - 18),
                extra_bits: 8,
            }),
            20..=21 => Ok(DistanceToken {
                base: 1025 + 512 * (value.0 - 20),
                extra_bits: 9,
            }),
            22..=23 => Ok(DistanceToken {
                base: 2049 + 1024 * (value.0 - 22),
                extra_bits: 10,
            }),
            24..=25 => Ok(DistanceToken {
                base: 4097 + 2048 * (value.0 - 24),
                extra_bits: 11,
            }),
            26..=27 => Ok(DistanceToken {
                base: 8193 + 4096 * (value.0 - 26),
                extra_bits: 12,
            }),
            28..=29 => Ok(DistanceToken {
                base: 16385 + 8192 * (value.0 - 28),
                extra_bits: 13,
            }),
            _ => Err(anyhow!("try from DistanceToken error")),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

const MAX_BITS: usize = 15;

pub struct HuffmanCodeWord(pub u16);

pub struct HuffmanCoding<T> {
    map: HashMap<BitSequence, T>,
}

impl<T> HuffmanCoding<T>
where
    T: Copy + TryFrom<HuffmanCodeWord, Error = anyhow::Error> + std::fmt::Debug,
{
    pub fn new(map: HashMap<BitSequence, T>) -> Self {
        Self { map }
    }

    #[allow(unused)]
    pub fn decode_symbol(&self, seq: BitSequence) -> Option<T> {
        self.map.get(&seq).copied()
    }

    pub fn read_symbol<U: BufRead>(&self, bit_reader: &mut BitReader<U>) -> Result<T> {
        let mut bit_sequence = BitSequence::new(0, 0);
        for _i in 0..MAX_BITS {
            let bit = bit_reader.read_bits(1)?;
            bit_sequence = bit_sequence.concat(bit);

            if let Some(&value) = self.map.get(&bit_sequence) {
                return Ok(value);
            }
        }
        bail!("read_symbol 2 type error")
    }

    pub fn from_lengths(code_lengths: &[u8]) -> Result<Self> {
        // algo from rfc
        let mut bl_count: [usize; MAX_BITS + 1] = [0; MAX_BITS + 1];
        let mut next_code: [usize; MAX_BITS + 1] = [0; MAX_BITS + 1];

        for &len in code_lengths {
            if usize::from(len) > MAX_BITS {
                bail!("from_lengths error")
            }
            bl_count[usize::from(len)] += 1;
        }

        let mut code = 0;
        bl_count[0] = 0;
        for bits in 1..=MAX_BITS {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        let mut map = HashMap::new();
        let mut n = 0;
        for &len in code_lengths {
            if len == 0 {
                n += 1;
                continue;
            }
            let value = T::try_from(HuffmanCodeWord(n))?;
            map.insert(
                BitSequence::new(next_code[usize::from(len)].try_into().unwrap(), len),
                value,
            );

            next_code[usize::from(len)] += 1;
            n += 1;
        }
        Ok(HuffmanCoding::new(map))
    }
}

////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Value(u16);

    impl TryFrom<HuffmanCodeWord> for Value {
        type Error = anyhow::Error;

        fn try_from(x: HuffmanCodeWord) -> Result<Self> {
            Ok(Self(x.0))
        }
    }

    #[test]
    fn from_lengths() -> Result<()> {
        let code = HuffmanCoding::<Value>::from_lengths(&[2, 3, 4, 3, 3, 4, 2])?;

        assert_eq!(
            code.decode_symbol(BitSequence::new(0b00, 2)),
            Some(Value(0)),
        );
        assert_eq!(
            code.decode_symbol(BitSequence::new(0b100, 3)),
            Some(Value(1)),
        );
        assert_eq!(
            code.decode_symbol(BitSequence::new(0b1110, 4)),
            Some(Value(2)),
        );
        assert_eq!(
            code.decode_symbol(BitSequence::new(0b101, 3)),
            Some(Value(3)),
        );
        assert_eq!(
            code.decode_symbol(BitSequence::new(0b110, 3)),
            Some(Value(4)),
        );
        assert_eq!(
            code.decode_symbol(BitSequence::new(0b1111, 4)),
            Some(Value(5)),
        );
        assert_eq!(
            code.decode_symbol(BitSequence::new(0b01, 2)),
            Some(Value(6)),
        );

        assert_eq!(code.decode_symbol(BitSequence::new(0b0, 1)), None);
        assert_eq!(code.decode_symbol(BitSequence::new(0b10, 2)), None);
        assert_eq!(code.decode_symbol(BitSequence::new(0b111, 3)), None,);

        Ok(())
    }

    #[test]
    fn read_symbol() -> Result<()> {
        let code = HuffmanCoding::<Value>::from_lengths(&[2, 3, 4, 3, 3, 4, 2])?;
        let mut data: &[u8] = &[0b10111001, 0b11001010, 0b11101101];
        let mut reader = BitReader::new(&mut data);

        assert_eq!(code.read_symbol(&mut reader)?, Value(1));
        assert_eq!(code.read_symbol(&mut reader)?, Value(2));
        assert_eq!(code.read_symbol(&mut reader)?, Value(3));
        assert_eq!(code.read_symbol(&mut reader)?, Value(6));
        assert_eq!(code.read_symbol(&mut reader)?, Value(0));
        assert_eq!(code.read_symbol(&mut reader)?, Value(2));
        assert_eq!(code.read_symbol(&mut reader)?, Value(4));
        assert!(code.read_symbol(&mut reader).is_err());

        Ok(())
    }

    #[test]
    fn from_lengths_with_zeros() -> Result<()> {
        let lengths = [3, 4, 5, 5, 0, 0, 6, 6, 4, 0, 6, 0, 7];
        let code = HuffmanCoding::<Value>::from_lengths(&lengths)?;
        let mut data: &[u8] = &[
            0b00100000, 0b00100001, 0b00010101, 0b10010101, 0b00110101, 0b00011101,
        ];
        let mut reader = BitReader::new(&mut data);

        assert_eq!(code.read_symbol(&mut reader)?, Value(0));
        assert_eq!(code.read_symbol(&mut reader)?, Value(1));
        assert_eq!(code.read_symbol(&mut reader)?, Value(2));
        assert_eq!(code.read_symbol(&mut reader)?, Value(3));
        assert_eq!(code.read_symbol(&mut reader)?, Value(6));
        assert_eq!(code.read_symbol(&mut reader)?, Value(7));
        assert_eq!(code.read_symbol(&mut reader)?, Value(8));
        assert_eq!(code.read_symbol(&mut reader)?, Value(10));
        assert_eq!(code.read_symbol(&mut reader)?, Value(12));
        assert!(code.read_symbol(&mut reader).is_err());

        Ok(())
    }

    #[test]
    fn from_lengths_additional() -> Result<()> {
        let lengths = [
            9, 10, 10, 8, 8, 8, 5, 6, 4, 5, 4, 5, 4, 5, 4, 4, 5, 4, 4, 5, 4, 5, 4, 5, 5, 5, 4, 6, 6,
        ];
        let code = HuffmanCoding::<Value>::from_lengths(&lengths)?;
        let mut data: &[u8] = &[
            0b11111000, 0b10111100, 0b01010001, 0b11111111, 0b00110101, 0b11111001, 0b11011111,
            0b11100001, 0b01110111, 0b10011111, 0b10111111, 0b00110100, 0b10111010, 0b11111111,
            0b11111101, 0b10010100, 0b11001110, 0b01000011, 0b11100111, 0b00000010,
        ];
        let mut reader = BitReader::new(&mut data);

        assert_eq!(code.read_symbol(&mut reader)?, Value(10));
        assert_eq!(code.read_symbol(&mut reader)?, Value(7));
        assert_eq!(code.read_symbol(&mut reader)?, Value(27));
        assert_eq!(code.read_symbol(&mut reader)?, Value(22));
        assert_eq!(code.read_symbol(&mut reader)?, Value(9));
        assert_eq!(code.read_symbol(&mut reader)?, Value(0));
        assert_eq!(code.read_symbol(&mut reader)?, Value(11));
        assert_eq!(code.read_symbol(&mut reader)?, Value(15));
        assert_eq!(code.read_symbol(&mut reader)?, Value(2));
        assert_eq!(code.read_symbol(&mut reader)?, Value(20));
        assert_eq!(code.read_symbol(&mut reader)?, Value(8));
        assert_eq!(code.read_symbol(&mut reader)?, Value(4));
        assert_eq!(code.read_symbol(&mut reader)?, Value(23));
        assert_eq!(code.read_symbol(&mut reader)?, Value(24));
        assert_eq!(code.read_symbol(&mut reader)?, Value(5));
        assert_eq!(code.read_symbol(&mut reader)?, Value(26));
        assert_eq!(code.read_symbol(&mut reader)?, Value(18));
        assert_eq!(code.read_symbol(&mut reader)?, Value(12));
        assert_eq!(code.read_symbol(&mut reader)?, Value(25));
        assert_eq!(code.read_symbol(&mut reader)?, Value(1));
        assert_eq!(code.read_symbol(&mut reader)?, Value(3));
        assert_eq!(code.read_symbol(&mut reader)?, Value(6));
        assert_eq!(code.read_symbol(&mut reader)?, Value(13));
        assert_eq!(code.read_symbol(&mut reader)?, Value(14));
        assert_eq!(code.read_symbol(&mut reader)?, Value(16));
        assert_eq!(code.read_symbol(&mut reader)?, Value(17));
        assert_eq!(code.read_symbol(&mut reader)?, Value(19));
        assert_eq!(code.read_symbol(&mut reader)?, Value(21));

        Ok(())
    }
}
