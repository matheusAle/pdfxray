//! The `/Encrypt` dictionary summarised for people: handler, version, algorithm, permissions and
//! whether the empty user password opens the file. Nothing is decrypted.

use pdfxray_raw::{Dict, Object};
use serde::Serialize;

use crate::crypto::{md5, rc4};
use crate::{Document, ObjRef};

/// The padding string of Algorithm 2 (ISO 32000-2 §7.6.4.3.2).
const PASSWORD_PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08, 0x2E,
    0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// What the user permission flags (`/P`) allow.
///
/// Bits 3 to 6 apply to every revision. Bits 9 to 12 only exist from revision 3 on; for revision 2
/// they are derived from the bits that governed those actions then (fill forms from annotate,
/// accessibility extraction from copy, assembly from modify, high-quality print from print).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Permissions {
    /// Print (bit 3).
    pub print: bool,
    /// Modify the content (bit 4).
    pub modify: bool,
    /// Copy or extract text and graphics (bit 5).
    pub copy: bool,
    /// Add or change annotations and form fields (bit 6).
    pub annotate: bool,
    /// Fill in existing form fields (bit 9).
    pub fill_forms: bool,
    /// Extract text for accessibility (bit 10).
    pub extract_accessibility: bool,
    /// Insert, rotate or delete pages and create bookmarks (bit 11).
    pub assemble: bool,
    /// Print a faithful, high-quality copy (bit 12).
    pub print_high_quality: bool,
}

impl Permissions {
    fn from_p(p: i32, revision: Option<u32>) -> Self {
        let bit = |n: u32| (p >> (n - 1)) & 1 == 1;
        let (print, modify, copy, annotate) = (bit(3), bit(4), bit(5), bit(6));
        if revision.is_some_and(|r| r < 3) {
            return Self {
                print,
                modify,
                copy,
                annotate,
                fill_forms: annotate,
                extract_accessibility: copy,
                assemble: modify,
                print_high_quality: print,
            };
        }
        Self {
            print,
            modify,
            copy,
            annotate,
            fill_forms: bit(9),
            extract_accessibility: bit(10),
            assemble: bit(11),
            print_high_quality: bit(12),
        }
    }
}

/// A summary of the encryption dictionary. See [`Document::encryption`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EncryptionInfo {
    /// The `/Encrypt` object, when the trailer refers to it (it can also be written inline).
    pub obj: Option<ObjRef>,
    /// `/Filter`: the security handler, `Standard` for password based encryption.
    pub filter: String,
    /// `/SubFilter`, when present.
    pub sub_filter: Option<String>,
    /// `/V`: the algorithm family.
    pub v: Option<u32>,
    /// `/R`: the revision of the standard handler.
    pub r: Option<u32>,
    /// Effective key length in bits.
    pub length_bits: u32,
    /// The cipher in words: `RC4 40-bit`, `RC4 128-bit`, `AESV2 128-bit`, `AESV3 256-bit`, or
    /// `None` when the crypt filter leaves streams in the clear, `unknown` otherwise.
    pub algorithm: String,
    /// What `/P` allows.
    pub permissions: Permissions,
    /// `/P` as an integer, when present.
    pub p: Option<i32>,
    /// Whether the document metadata stream is encrypted (`/EncryptMetadata`, default true).
    pub metadata_encrypted: bool,
    /// `Some(true)` when the empty user password opens the file (no password is needed to read
    /// it), `Some(false)` when it does not (the user password is not empty, or the dictionary is
    /// not a valid one), `None` when it cannot be told without decrypting: handlers other than
    /// `Standard` and revisions 5 and 6 (AES-256), whose check needs SHA-2 and AES.
    pub user_password_empty: Option<bool>,
}

impl Document {
    /// The encryption dictionary summarised, or `None` for an unencrypted file.
    ///
    /// The empty-password check is pure key derivation (Algorithms 2, 4 and 5 of ISO 32000-1
    /// §7.6.3.3, revisions 2 to 4: MD5 and RC4 over `/O`, `/P`, the first `/ID` string and the
    /// padded empty password, compared with `/U`). It decrypts no content.
    pub fn encryption(&self) -> Option<EncryptionInfo> {
        let dict = self.raw.encryption()?;
        let get = |key: &[u8]| {
            dict.get(key)
                .map(|v| self.raw.resolve(&v.value).into_owned())
                .filter(|v| *v != Object::Null)
        };
        let int = |key: &[u8]| get(key).and_then(|v| v.as_int());
        let name = |key: &[u8]| get(key).and_then(|v| v.as_name().map(pdfxray_raw::Name::to_string_lossy));
        let to_u32 = |v: i64| u32::try_from(v).ok();

        let v = int(b"V").and_then(to_u32);
        let r = int(b"R").and_then(to_u32);
        // `/P` is a 32-bit pattern; some writers store it unsigned.
        let p = int(b"P")
            .and_then(|p| u32::try_from(p & 0xFFFF_FFFF).ok())
            .map(|u| i32::from_ne_bytes(u.to_ne_bytes()));
        let (algorithm, length_bits) = self.cipher(dict, v, int(b"Length").and_then(to_u32));
        let filter = name(b"Filter").unwrap_or_else(|| "(none)".to_owned());
        let metadata_encrypted = !matches!(get(b"EncryptMetadata"), Some(Object::Bool(false)));
        let user_password_empty = (filter == "Standard")
            .then(|| self.empty_user_password(dict, r, length_bits, p, metadata_encrypted))
            .flatten();
        Some(EncryptionInfo {
            obj: self
                .raw
                .trailer()
                .and_then(|t| t.get(b"Encrypt"))
                .and_then(|e| e.value.as_ref_obj()),
            filter,
            sub_filter: name(b"SubFilter"),
            v,
            r,
            length_bits,
            algorithm,
            permissions: Permissions::from_p(p.unwrap_or(-1), r),
            p,
            metadata_encrypted,
            user_password_empty,
        })
    }

    /// The cipher in words and the key length in bits.
    fn cipher(&self, dict: &Dict, v: Option<u32>, length: Option<u32>) -> (String, u32) {
        let rc4 = |bits: u32| (format!("RC4 {bits}-bit"), bits);
        match v {
            Some(1) => rc4(40),
            Some(2) => rc4(length.unwrap_or(40)),
            Some(3) => ("unpublished algorithm".to_owned(), length.unwrap_or(40)),
            Some(4) => self.crypt_filter(dict, length),
            Some(5) => ("AESV3 256-bit".to_owned(), 256),
            _ => ("unknown".to_owned(), length.unwrap_or(40)),
        }
    }

    /// The cipher of the stream crypt filter (`/StmF`) of a V 4 dictionary.
    fn crypt_filter(&self, dict: &Dict, length: Option<u32>) -> (String, u32) {
        let resolve = |o: &Object| self.raw.resolve(o).into_owned();
        let stmf = dict
            .get(b"StmF")
            .map(|v| resolve(&v.value))
            .and_then(|v| v.as_name().map(pdfxray_raw::Name::to_string_lossy))
            .unwrap_or_else(|| "Identity".to_owned());
        if stmf == "Identity" {
            return ("None".to_owned(), 0);
        }
        let filter = dict
            .get(b"CF")
            .map(|v| resolve(&v.value))
            .and_then(|cf| cf.as_dict()?.get(stmf.as_bytes()).map(|f| resolve(&f.value)))
            .and_then(|f| f.as_dict().cloned());
        let method = filter
            .as_ref()
            .and_then(|f| f.get(b"CFM"))
            .map(|m| resolve(&m.value))
            .and_then(|m| m.as_name().map(pdfxray_raw::Name::to_string_lossy));
        // A crypt filter's /Length is in bytes in practice, in bits by the book.
        let bits = filter
            .as_ref()
            .and_then(|f| f.get(b"Length"))
            .and_then(|l| resolve(&l.value).as_int())
            .and_then(|l| u32::try_from(l).ok())
            .map(|l| if l <= 32 { l * 8 } else { l })
            .or(length)
            .unwrap_or(128);
        match method.as_deref() {
            Some("AESV2") => ("AESV2 128-bit".to_owned(), 128),
            Some("AESV3") => ("AESV3 256-bit".to_owned(), 256),
            Some("V2") => (format!("RC4 {bits}-bit"), bits),
            Some("None") => ("None".to_owned(), 0),
            _ => ("unknown".to_owned(), bits),
        }
    }

    /// Whether the empty user password reproduces `/U` (revisions 2 to 4).
    fn empty_user_password(
        &self,
        dict: &Dict,
        revision: Option<u32>,
        length_bits: u32,
        p: Option<i32>,
        metadata_encrypted: bool,
    ) -> Option<bool> {
        let revision = revision.filter(|r| (2..=4).contains(r))?;
        let string = |key: &[u8]| {
            let value = self.raw.resolve(&dict.get(key)?.value).into_owned();
            match value {
                Object::String(s) => Some(s.bytes),
                _ => None,
            }
        };
        let owner = string(b"O")?;
        let user = string(b"U")?;
        let id0 = self
            .raw
            .trailer()
            .and_then(|t| t.get(b"ID"))
            .map(|id| self.raw.resolve(&id.value).into_owned())
            .and_then(|id| match id.as_array()?.first()?.value.clone() {
                Object::String(s) => Some(s.bytes),
                _ => None,
            })
            .unwrap_or_default();
        let key_len = if revision == 2 {
            5
        } else {
            usize::try_from(length_bits / 8).ok()?.clamp(5, 16)
        };
        let key = file_key(&owner, p?, &id0, revision, key_len, metadata_encrypted);
        let expected = user_entry(&key, &id0, revision);
        let compared = if revision == 2 { 32 } else { 16 };
        Some(user.get(..compared)? == expected.get(..compared)?)
    }
}

/// Algorithm 2: the file encryption key for the empty user password.
fn file_key(
    owner: &[u8],
    p: i32,
    id0: &[u8],
    revision: u32,
    key_len: usize,
    metadata_encrypted: bool,
) -> Vec<u8> {
    let mut input = PASSWORD_PAD.to_vec();
    input.extend_from_slice(owner.get(..32).unwrap_or(owner));
    input.extend_from_slice(&p.to_le_bytes());
    input.extend_from_slice(id0);
    if revision >= 4 && !metadata_encrypted {
        input.extend_from_slice(&[0xFF; 4]);
    }
    let mut digest = md5(&input);
    if revision >= 3 {
        for _ in 0..50 {
            digest = md5(digest.get(..key_len).unwrap_or(&digest));
        }
    }
    digest.get(..key_len).unwrap_or(&digest).to_vec()
}

/// Algorithms 4 and 5: the `/U` value the key produces.
fn user_entry(key: &[u8], id0: &[u8], revision: u32) -> Vec<u8> {
    if revision == 2 {
        return rc4(key, &PASSWORD_PAD);
    }
    let mut seed = PASSWORD_PAD.to_vec();
    seed.extend_from_slice(id0);
    let mut value = rc4(key, &md5(&seed));
    for round in 1..=19_u8 {
        let round_key: Vec<u8> = key.iter().map(|b| b ^ round).collect();
        value = rc4(&round_key, &value);
    }
    value
}
