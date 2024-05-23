#![forbid(unsafe_code)]

use std::cmp::min;
use std::collections::VecDeque;
use std::io::{self, Write};

use anyhow::{anyhow, Context, Result};
use crc::Digest;
use crc::CRC_32_ISO_HDLC;

////////////////////////////////////////////////////////////////////////////////

const HISTORY_SIZE: usize = 32768;
pub const ALGORITHM: crc::Crc<u32> = crc::Crc::<u32>::new(&CRC_32_ISO_HDLC);

pub struct TrackingWriter<T> {
    inner: T,
    buffer: VecDeque<u8>,
    byte_counter: usize,
    digest: Digest<'static, u32>,
}

impl<T: Write> Write for TrackingWriter<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let to_write = self.inner.write(buf)?;
        self.byte_counter += to_write;
        self.digest.update((*buf).get(0..to_write).unwrap());
        self.buffer.extend((*buf).get(0..to_write).unwrap());
        if self.buffer.len() >= HISTORY_SIZE {
            self.buffer.drain(0..self.buffer.len() - HISTORY_SIZE);
        }
        Ok(to_write)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<T: Write> TrackingWriter<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            buffer: VecDeque::<u8>::new(),
            byte_counter: 0,
            digest: ALGORITHM.digest(),
        }
    }

    pub fn clear(&mut self) -> Result<()> {
        self.buffer = VecDeque::<u8>::new();
        self.byte_counter = 0;
        self.digest = ALGORITHM.digest();
        Ok(())
    }

    /// Write a sequence of `len` bytes written `dist` bytes ago.
    pub fn write_previous(&mut self, dist: usize, len: usize) -> Result<()> {
        if self.buffer.len() < dist {
            return Err(anyhow!("bad len in write previous"));
        }
        self.write_all(
            &(self
                .buffer
                .range(
                    self.buffer.len() - dist
                        ..min(self.buffer.len(), self.buffer.len() - dist + len),
                )
                .copied()
                .cycle()
                .take(len)
                .collect::<Vec<_>>()),
        )
        .context("write all failed")?;
        Ok(())
    }

    pub fn byte_count(&self) -> u32 {
        self.byte_counter as u32
    }

    pub fn crc32(&mut self) -> u32 {
        self.digest.clone().finalize()
    }
}

////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::WriteBytesExt;

    #[test]
    fn write() -> Result<()> {
        let mut buf: &mut [u8] = &mut [0u8; 10];
        let mut writer = TrackingWriter::new(&mut buf);

        assert_eq!(writer.write(&[1, 2, 3, 4])?, 4);
        assert_eq!(writer.byte_count(), 4);

        assert_eq!(writer.write(&[4, 8, 15, 16, 23])?, 5);
        assert_eq!(writer.byte_count(), 9);

        assert_eq!(writer.write(&[0, 0, 123])?, 1);
        assert_eq!(writer.byte_count(), 10);

        assert_eq!(writer.write(&[42, 124, 234, 27])?, 0);
        assert_eq!(writer.byte_count(), 10);
        assert_eq!(writer.crc32(), 2992191065);

        Ok(())
    }

    #[test]
    fn write_previous() -> Result<()> {
        let mut buf: &mut [u8] = &mut [0u8; 512];
        let mut writer = TrackingWriter::new(&mut buf);

        for i in 0..=255 {
            writer.write_u8(i)?;
        }

        writer.write_previous(192, 128)?;
        assert_eq!(writer.byte_count(), 384);

        assert!(writer.write_previous(10000, 20).is_err());
        assert_eq!(writer.byte_count(), 384);

        assert!(writer.write_previous(256, 256).is_err());
        assert_eq!(writer.byte_count(), 512);

        assert!(writer.write_previous(1, 1).is_err());
        assert_eq!(writer.byte_count(), 512);
        assert_eq!(writer.crc32(), 2733545866);

        Ok(())
    }
}
