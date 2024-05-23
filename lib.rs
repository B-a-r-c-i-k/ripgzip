#![forbid(unsafe_code)]

use std::{
    // error,
    io::{BufRead, Write},
};

use anyhow::Result;
use tracking_writer::TrackingWriter;

use crate::gzip::GzipReader;
use bit_reader::BitReader;
use deflate::DeflateReader;

mod bit_reader;
mod deflate;
mod gzip;
mod huffman_coding;
mod tracking_writer;

pub fn decompress<R: BufRead, W: Write>(input: R, output: W) -> Result<()> {
    let mut deflate = DeflateReader::new(BitReader::new(input), TrackingWriter::new(output));
    let mut gzip_reader = GzipReader::new(deflate.get_input());
    while !gzip_reader.is_empty()? {
        match gzip_reader.parse_header() {
            Ok(()) => loop {
                match deflate.next_block() {
                    Ok(x) => {
                        if x {
                            break;
                        } else {
                            continue;
                        }
                    }
                    Err(error) => {
                        return Err(error);
                    }
                }
            },
            Err(error) => {
                return Err(error);
            }
        }
        gzip_reader = GzipReader::new(deflate.get_input());
        let (crc32, isize) = gzip_reader.read_crc32_and_isize()?;
        deflate.check_crc32_and_isize(crc32, isize)?;
        deflate.output()?;
        gzip_reader = GzipReader::new(deflate.get_input());
    }
    Ok(())
}
