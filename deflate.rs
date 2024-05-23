#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

use anyhow::{bail, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::huffman_coding::HuffmanCoding;
use crate::huffman_coding::{DistanceToken, LitLenToken};
use crate::tracking_writer::TrackingWriter;
use crate::{
    bit_reader::BitReader,
    huffman_coding::{decode_dynamic_tree, decode_fixed_trees},
};

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct BlockHeader {
    pub is_final: bool,
    pub compression_type: CompressionType,
}

#[derive(Debug)]
pub enum CompressionType {
    Uncompressed = 0,
    FixedTree = 1,
    DynamicTree = 2,
    Reserved = 3,
}

////////////////////////////////////////////////////////////////////////////////

pub struct DeflateReader<T, W> {
    bit_reader: BitReader<T>,
    writer: TrackingWriter<W>,
}

impl<T: BufRead, W: Write> DeflateReader<T, W> {
    pub fn new(bit_reader: BitReader<T>, writer: TrackingWriter<W>) -> Self {
        Self { bit_reader, writer }
    }

    pub fn next_block(&mut self) -> Result<bool> {
        let bfinal = self.bit_reader.read_bits(1).context("bfinal read")?.bits();
        let btype = self.bit_reader.read_bits(2).context("btype read")?.bits();

        let cm = match btype {
            0 => CompressionType::Uncompressed,
            1 => CompressionType::FixedTree,
            2 => CompressionType::DynamicTree,
            3 => CompressionType::Reserved,
            _ => unreachable!("reach bad btype"),
        };
        let block_header = BlockHeader {
            is_final: bfinal != 0,
            compression_type: cm,
        };
        self.read_data(block_header)
    }

    pub fn read_data(&mut self, block_header: BlockHeader) -> Result<bool> {
        match block_header.compression_type {
            CompressionType::Uncompressed => {
                let reader = self.bit_reader.borrow_reader_from_boundary();
                let len = reader.read_u16::<LittleEndian>().context("LEN")?;
                let nlen = reader.read_u16::<LittleEndian>().context("NLEN")?;
                if len != !nlen {
                    bail!("nlen check failed")
                }

                let mut buffer: Vec<u8> = vec![0; len.into()];
                reader
                    .read_exact(&mut buffer)
                    .context("uncompressed read")?;
                self.writer
                    .write_all(&buffer)
                    .context("uncompressed write")?;
                Ok(block_header.is_final)
            }
            CompressionType::FixedTree => {
                let (letlentoken, distancetoken) =
                    decode_fixed_trees().context("fixed tree failed")?;
                match self.decode_by_tokens(letlentoken, distancetoken) {
                    Ok(()) => Ok(block_header.is_final),
                    _ => {
                        bail!("parse after fixed tree failed")
                    }
                }
            }
            CompressionType::DynamicTree => {
                let (letlentoken, distancetoken) =
                    decode_dynamic_tree(&mut self.bit_reader).context("dynamic tree failed")?;
                match self.decode_by_tokens(letlentoken, distancetoken) {
                    Ok(()) => Ok(block_header.is_final),
                    _ => {
                        bail!("parse after dynamic tree failed")
                    }
                }
            }
            _ => {
                bail!("unsupported block type")
            }
        }
    }

    fn decode_by_tokens(
        &mut self,
        letlentoken: HuffmanCoding<LitLenToken>,
        distancetoken: HuffmanCoding<DistanceToken>,
    ) -> Result<()> {
        loop {
            match letlentoken.read_symbol(&mut self.bit_reader)? {
                LitLenToken::Literal(symbol) => {
                    self.writer.write_u8(symbol)?;
                }
                LitLenToken::EndOfBlock => break,
                LitLenToken::Length { base, extra_bits } => {
                    let len = self.bit_reader.read_bits(extra_bits)?.bits() + base;

                    let distancetoken = distancetoken.read_symbol(&mut self.bit_reader)?;
                    let dist = self.bit_reader.read_bits(distancetoken.extra_bits)?.bits()
                        + distancetoken.base;
                    self.writer.write_previous(dist.into(), len.into())?;
                }
            }
        }
        Ok(())
    }

    pub fn get_input(&mut self) -> &mut T {
        self.bit_reader.borrow_reader_from_boundary()
    }

    pub fn output(&mut self) -> Result<()> {
        self.writer.flush()?;
        self.writer.clear()?;
        Ok(())
    }

    pub fn check_crc32_and_isize(&mut self, crc32: u32, isize: u32) -> Result<()> {
        if crc32 != self.writer.crc32() {
            bail!("crc32 check failed")
        }
        if isize != self.writer.byte_count() {
            bail!("length check failed")
        }
        Ok(())
    }
}

// TODO: your code goes here.
