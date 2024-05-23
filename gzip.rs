#![forbid(unsafe_code)]

use std::io::BufRead;

use anyhow::{bail, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use crc::Crc;

////////////////////////////////////////////////////////////////////////////////

const ID1: u8 = 0x1f;
const ID2: u8 = 0x8b;

const CM_DEFLATE: u8 = 8;

const FTEXT_OFFSET: u8 = 0;
const FHCRC_OFFSET: u8 = 1;
const FEXTRA_OFFSET: u8 = 2;
const FNAME_OFFSET: u8 = 3;
const FCOMMENT_OFFSET: u8 = 4;

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct MemberHeader {
    pub compression_method: CompressionMethod,
    pub modification_time: u32,
    pub extra: Option<Vec<u8>>,
    pub name: Option<String>,
    pub comment: Option<String>,
    pub extra_flags: u8,
    pub os: u8,
    pub has_crc: bool,
    pub is_text: bool,
}

impl MemberHeader {
    pub fn crc16(&self) -> u16 {
        let crc = Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        let mut digest = crc.digest();

        digest.update(&[ID1, ID2, self.compression_method.into(), self.flags().0]);
        digest.update(&self.modification_time.to_le_bytes());
        digest.update(&[self.extra_flags, self.os]);

        if let Some(extra) = &self.extra {
            digest.update(&(extra.len() as u16).to_le_bytes());
            digest.update(extra);
        }

        if let Some(name) = &self.name {
            digest.update(name.as_bytes());
            digest.update(&[0]);
        }

        if let Some(comment) = &self.comment {
            digest.update(comment.as_bytes());
            digest.update(&[0]);
        }

        (digest.finalize() & 0xffff) as u16
    }

    pub fn flags(&self) -> MemberFlags {
        let mut flags = MemberFlags(0);
        flags.set_is_text(self.is_text);
        flags.set_has_crc(self.has_crc);
        flags.set_has_extra(self.extra.is_some());
        flags.set_has_name(self.name.is_some());
        flags.set_has_comment(self.comment.is_some());
        flags
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum CompressionMethod {
    Deflate,
    Unknown(u8),
}

impl From<u8> for CompressionMethod {
    fn from(value: u8) -> Self {
        match value {
            CM_DEFLATE => Self::Deflate,
            x => Self::Unknown(x),
        }
    }
}

impl From<CompressionMethod> for u8 {
    fn from(method: CompressionMethod) -> u8 {
        match method {
            CompressionMethod::Deflate => CM_DEFLATE,
            CompressionMethod::Unknown(x) => x,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct MemberFlags(u8);

#[allow(unused)]
impl MemberFlags {
    fn bit(&self, n: u8) -> bool {
        (self.0 >> n) & 1 != 0
    }

    fn set_bit(&mut self, n: u8, value: bool) {
        if value {
            self.0 |= 1 << n;
        } else {
            self.0 &= !(1 << n);
        }
    }

    pub fn is_text(&self) -> bool {
        self.bit(FTEXT_OFFSET)
    }

    pub fn set_is_text(&mut self, value: bool) {
        self.set_bit(FTEXT_OFFSET, value)
    }

    pub fn has_crc(&self) -> bool {
        self.bit(FHCRC_OFFSET)
    }

    pub fn set_has_crc(&mut self, value: bool) {
        self.set_bit(FHCRC_OFFSET, value)
    }

    pub fn has_extra(&self) -> bool {
        self.bit(FEXTRA_OFFSET)
    }

    pub fn set_has_extra(&mut self, value: bool) {
        self.set_bit(FEXTRA_OFFSET, value)
    }

    pub fn has_name(&self) -> bool {
        self.bit(FNAME_OFFSET)
    }

    pub fn set_has_name(&mut self, value: bool) {
        self.set_bit(FNAME_OFFSET, value)
    }

    pub fn has_comment(&self) -> bool {
        self.bit(FCOMMENT_OFFSET)
    }

    pub fn set_has_comment(&mut self, value: bool) {
        self.set_bit(FCOMMENT_OFFSET, value)
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct MemberFooter {
    pub data_crc32: u32,
    pub data_size: u32,
}

////////////////////////////////////////////////////////////////////////////////

pub struct GzipReader<T> {
    reader: T,
}

impl<T: BufRead> GzipReader<T> {
    pub fn new(reader: T) -> Self {
        Self { reader }
    }

    pub fn parse_header(mut self) -> Result<()> {
        let id1 = self.reader.read_u8()?;
        let id2 = self.reader.read_u8()?;
        if id1 != ID1 || id2 != ID2 {
            bail!("wrong id values")
        }

        let cm = CompressionMethod::from(self.reader.read_u8().context("CM")?);

        let flg = MemberFlags(self.reader.read_u8().context("FLG")?);
        let mtime = self.reader.read_u32::<LittleEndian>().context("MTIME")?;
        let xfl = self.reader.read_u8().context("XFL")?;
        let os = self.reader.read_u8().context("OS")?;

        let mut extra: Option<Vec<u8>> = None;

        if flg.has_extra() {
            let xlen = self.reader.read_u16::<LittleEndian>().context("XLEN")?;
            let mut buffer: Vec<u8> = vec![0; xlen.into()];
            self.reader
                .read_exact(&mut buffer)
                .context("extra read fail")?;
            extra = Some(buffer);
        }

        let mut name: Option<String> = None;

        if flg.has_name() {
            let mut buffer: Vec<u8> = Vec::new();
            self.reader
                .read_until(0, &mut buffer)
                .context("name read fail")?;
            name = Some(String::from_utf8(buffer)?);
        }

        let mut comment: Option<String> = None;

        if flg.has_comment() {
            let mut buffer: Vec<u8> = Vec::new();
            self.reader
                .read_until(0, &mut buffer)
                .context("name read fail")?;
            comment = Some(String::from_utf8(buffer)?);
        }

        let mut crc: bool = false;
        let mut crc_value = 0;

        if flg.has_crc() {
            crc_value = self.reader.read_u16::<LittleEndian>().context("XLEN")?;
            crc = true;
        }

        let is_text: bool = flg.is_text();

        let member_header = MemberHeader {
            compression_method: cm,
            modification_time: mtime,
            extra,
            name,
            comment,
            extra_flags: xfl,
            os,
            has_crc: crc,
            is_text,
        };

        if crc && member_header.crc16() != crc_value {
            bail!("header crc16 check failed")
        }
        match cm {
            CompressionMethod::Deflate => Ok(()),
            _ => bail!("unsupported compression method"),
        }
    }

    pub fn read_crc32_and_isize(mut self) -> Result<(u32, u32)> {
        Ok((
            self.reader.read_u32::<LittleEndian>()?,
            self.reader.read_u32::<LittleEndian>()?,
        ))
    }

    pub fn is_empty(&mut self) -> Result<bool> {
        Ok(self.reader.fill_buf()?.is_empty())
    }
}

////////////////////////////////////////////////////////////////////////////////
