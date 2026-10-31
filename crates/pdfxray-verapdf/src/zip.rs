//! Minimal, defensive ZIP reader: extracts one named entry of a (non-zip64) archive.
//!
//! The installer archive is already SHA-256 verified before it gets here; the checks below
//! (bounds, sizes, CRC-32) protect against corrupt data and keep the reader panic free.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use flate2::Crc;
use flate2::read::DeflateDecoder;
use pdfxray_core::numeric::u64_from_usize;

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const EOCD_MIN: u64 = 22;
const MAX_COMMENT: u64 = 65_535;

/// Reasons an entry could not be extracted.
#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    /// Reading the archive failed.
    #[error("reading archive failed: {0}")]
    Io(#[from] std::io::Error),
    /// The archive structure is invalid or unsupported.
    #[error("invalid archive: {0}")]
    Invalid(&'static str),
    /// No entry with the requested name exists.
    #[error("archive has no entry `{0}`")]
    MissingEntry(String),
    /// The entry would exceed the allowed size.
    #[error("entry `{name}` is larger than the allowed {limit} bytes")]
    TooLarge {
        /// Entry name.
        name: String,
        /// Allowed size.
        limit: u64,
    },
    /// The extracted data does not match the stored checksum.
    #[error("entry `{0}` failed its CRC-32 check")]
    CrcMismatch(String),
}

struct Entry {
    method: u16,
    crc: u32,
    compressed: u64,
    uncompressed: u64,
    local_offset: u64,
}

/// Extracts the entry called exactly `name` from `archive` into `out`, never writing more than
/// `max_size` bytes.
pub(crate) fn extract_entry<A: Read + Seek, W: Write>(
    archive: &mut A,
    name: &str,
    max_size: u64,
    out: &mut W,
) -> Result<(), ZipError> {
    let entry = find_entry(archive, name)?;
    if entry.uncompressed > max_size {
        return Err(ZipError::TooLarge {
            name: name.to_owned(),
            limit: max_size,
        });
    }
    archive.seek(SeekFrom::Start(entry.local_offset))?;
    let mut header = [0u8; 30];
    archive.read_exact(&mut header)?;
    if u32_at(&header, 0) != LOCAL_SIGNATURE {
        return Err(ZipError::Invalid("bad local header signature"));
    }
    let skip = u64::from(u16_at(&header, 26)) + u64::from(u16_at(&header, 28));
    archive.seek(SeekFrom::Current(
        i64::try_from(skip).map_err(|_| ZipError::Invalid("offset overflow"))?,
    ))?;

    let raw = archive.take(entry.compressed);
    let mut crc = Crc::new();
    let mut written = 0u64;
    let mut copy = |reader: &mut dyn Read| -> Result<(), ZipError> {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            written += u64_from_usize(n);
            if written > entry.uncompressed.min(max_size) {
                return Err(ZipError::TooLarge {
                    name: name.to_owned(),
                    limit: max_size,
                });
            }
            crc.update(&buf[..n]);
            out.write_all(&buf[..n])?;
        }
    };
    match entry.method {
        0 => copy(&mut { raw })?,
        8 => copy(&mut DeflateDecoder::new(raw))?,
        _ => return Err(ZipError::Invalid("unsupported compression method")),
    }
    if written != entry.uncompressed {
        return Err(ZipError::Invalid(
            "entry size does not match the central directory",
        ));
    }
    if crc.sum() != entry.crc {
        return Err(ZipError::CrcMismatch(name.to_owned()));
    }
    Ok(())
}

fn find_entry<A: Read + Seek>(archive: &mut A, name: &str) -> Result<Entry, ZipError> {
    let len = archive.seek(SeekFrom::End(0))?;
    if len < EOCD_MIN {
        return Err(ZipError::Invalid("too short"));
    }
    let tail_len = len.min(EOCD_MIN + MAX_COMMENT);
    archive.seek(SeekFrom::Start(len - tail_len))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).map_err(|_| ZipError::Invalid("tail"))?];
    archive.read_exact(&mut tail)?;
    let eocd = (0..=tail.len() - 22)
        .rev()
        .find(|&at| u32_at(&tail, at) == EOCD_SIGNATURE)
        .ok_or(ZipError::Invalid("no end of central directory"))?;
    let count = u64::from(u16_at(&tail, eocd + 10));
    let dir_size = u64::from(u32_at(&tail, eocd + 12));
    let dir_offset = u64::from(u32_at(&tail, eocd + 16));
    if count == 0xFFFF || dir_size == 0xFFFF_FFFF || dir_offset == 0xFFFF_FFFF {
        return Err(ZipError::Invalid("zip64 archives are not supported"));
    }
    if dir_offset.checked_add(dir_size).is_none_or(|end| end > len) {
        return Err(ZipError::Invalid("central directory out of bounds"));
    }
    archive.seek(SeekFrom::Start(dir_offset))?;
    let mut dir = vec![0u8; usize::try_from(dir_size).map_err(|_| ZipError::Invalid("directory size"))?];
    archive.read_exact(&mut dir)?;

    let mut at = 0usize;
    for _ in 0..count {
        let fixed = dir
            .get(at..at + 46)
            .ok_or(ZipError::Invalid("truncated central directory"))?;
        if u32_at(fixed, 0) != CENTRAL_SIGNATURE {
            return Err(ZipError::Invalid("bad central directory signature"));
        }
        let name_len = usize::from(u16_at(fixed, 28));
        let extra_len = usize::from(u16_at(fixed, 30));
        let comment_len = usize::from(u16_at(fixed, 32));
        let entry_name = dir
            .get(at + 46..at + 46 + name_len)
            .ok_or(ZipError::Invalid("truncated entry name"))?;
        if entry_name == name.as_bytes() {
            return Ok(Entry {
                method: u16_at(fixed, 10),
                crc: u32_at(fixed, 16),
                compressed: u64::from(u32_at(fixed, 20)),
                uncompressed: u64::from(u32_at(fixed, 24)),
                local_offset: u64::from(u32_at(fixed, 42)),
            });
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Err(ZipError::MissingEntry(name.to_owned()))
}

/// Convenience wrapper: extract `name` from the archive file at `archive` into `dest`.
pub(crate) fn extract_entry_to_file(
    archive: &std::path::Path,
    name: &str,
    max_size: u64,
    dest: &std::path::Path,
) -> Result<(), ZipError> {
    let mut input = File::open(archive)?;
    let mut output = File::create(dest)?;
    extract_entry(&mut input, name, max_size, &mut output)?;
    output.flush()?;
    Ok(())
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::DeflateEncoder;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    fn le16(n: usize) -> [u8; 2] {
        u16::try_from(n).expect("fits u16").to_le_bytes()
    }

    fn le32(n: usize) -> [u8; 4] {
        u32::try_from(n).expect("fits u32").to_le_bytes()
    }

    /// Builds a zip with the given `(name, method, data)` entries.
    fn build_zip(entries: &[(&str, u16, &[u8])], comment: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut directory = Vec::new();
        for (name, method, data) in entries {
            let mut crc = Crc::new();
            crc.update(data);
            let stored: Vec<u8> = if *method == 8 {
                let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
                enc.write_all(data).expect("deflate");
                enc.finish().expect("finish")
            } else {
                data.to_vec()
            };
            let offset = le32(out.len());
            out.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&crc.sum().to_le_bytes());
            out.extend_from_slice(&le32(stored.len()));
            out.extend_from_slice(&le32(data.len()));
            out.extend_from_slice(&le16(name.len()));
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&stored);

            directory.extend_from_slice(&CENTRAL_SIGNATURE.to_le_bytes());
            directory.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            directory.extend_from_slice(&method.to_le_bytes());
            directory.extend_from_slice(&[0, 0, 0, 0]);
            directory.extend_from_slice(&crc.sum().to_le_bytes());
            directory.extend_from_slice(&le32(stored.len()));
            directory.extend_from_slice(&le32(data.len()));
            directory.extend_from_slice(&le16(name.len()));
            directory.extend_from_slice(&[0; 12]);
            directory.extend_from_slice(&offset);
            directory.extend_from_slice(name.as_bytes());
        }
        let dir_offset = le32(out.len());
        out.extend_from_slice(&directory);
        out.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&le16(entries.len()));
        out.extend_from_slice(&le16(entries.len()));
        out.extend_from_slice(&le32(directory.len()));
        out.extend_from_slice(&dir_offset);
        out.extend_from_slice(&le16(comment.len()));
        out.extend_from_slice(comment);
        out
    }

    fn extract(zip: Vec<u8>, name: &str, max: u64) -> Result<Vec<u8>, ZipError> {
        let mut out = Vec::new();
        extract_entry(&mut Cursor::new(zip), name, max, &mut out)?;
        Ok(out)
    }

    #[test]
    fn extracts_stored_and_deflated_entries() {
        let payload = b"installer bytes ".repeat(200);
        let zip = build_zip(
            &[
                ("dir/", 0, b""),
                ("dir/a.txt", 0, b"stored"),
                ("dir/b.jar", 8, &payload),
            ],
            b"a comment",
        );
        assert_eq!(
            extract(zip.clone(), "dir/a.txt", 1 << 20).expect("stored"),
            b"stored".to_vec()
        );
        assert_eq!(extract(zip, "dir/b.jar", 1 << 20).expect("deflated"), payload);
    }

    #[test]
    fn missing_oversized_and_corrupt_entries_are_rejected() {
        let payload = vec![7u8; 4096];
        let zip = build_zip(&[("x.bin", 8, &payload)], b"");
        assert!(
            matches!(extract(zip.clone(), "y.bin", 1 << 20), Err(ZipError::MissingEntry(n)) if n == "y.bin")
        );
        assert!(matches!(
            extract(zip.clone(), "x.bin", 100),
            Err(ZipError::TooLarge { limit: 100, .. })
        ));

        let mut corrupt = build_zip(&[("x.bin", 0, b"hello world")], b"");
        let at = corrupt
            .windows(11)
            .position(|w| w == b"hello world")
            .expect("payload present");
        corrupt[at] ^= 0xFF;
        assert!(matches!(
            extract(corrupt, "x.bin", 1 << 20),
            Err(ZipError::CrcMismatch(_))
        ));
    }

    #[test]
    fn garbage_is_invalid_not_a_panic() {
        for bytes in [
            vec![],
            vec![0u8; 10],
            [0x50, 0x4b, 0x05, 0x06].repeat(10),
            b"not a zip at all, definitely not".to_vec(),
        ] {
            assert!(extract(bytes, "x", 10).is_err());
        }
    }
}
