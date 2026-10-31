//! The Ed25519 (`EdDSA`) keys that sign updates, in Sparkle's format.
//!
//! Sparkle's current key format is plain base64: the private key is the 32-byte Ed25519 seed (what
//! `generate_keys -x` exports for keys made by Sparkle 2) and the public key is the 32-byte
//! Ed25519 public key (`SUPublicEDKey`). Signatures are the 64-byte RFC 8032 signature of the whole
//! archive, base64 (`sparkle:edSignature`). Ed25519 signatures are deterministic, so a signature
//! made here is byte for byte the one Sparkle's `sign_update` makes with the same key.
//!
//! Keys are generated here rather than with `generate_keys`, which only stores them in the login
//! keychain; the private key is printed once and never written to disk by xtask.

use std::fmt;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ring::rand::{SecureRandom as _, SystemRandom};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair as _, UnparsedPublicKey};

/// `SUPublicEDKey`: the public half of the key that signs pdfxray's updates. Installed copies
/// accept an update only when its archive is signed by the matching private key.
// The production key (2026-10-31). Its private half is the CI secret `SPARKLE_ED_PRIVATE_KEY`
// and lives in the maintainer's password manager; changing this breaks updates for installed copies.
pub const PUBLIC_ED_KEY: &str = "RPw8jqLIH6B+Hsq4lkHWnQKNQ/PRZBBpFJEHeq9pnnE=";

/// Environment variable that holds the private key (base64 seed) for `sparkle appcast`.
pub const ENV_PRIVATE_KEY: &str = "SPARKLE_ED_PRIVATE_KEY";

/// Length of an Ed25519 seed and of a public key.
const KEY_LEN: usize = 32;

/// An Ed25519 key pair. Its `Debug` output shows the public key only.
pub struct KeyPair {
    seed: [u8; KEY_LEN],
    pair: Ed25519KeyPair,
}

impl KeyPair {
    /// A new random key pair from the operating system's random number generator.
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; KEY_LEN];
        SystemRandom::new()
            .fill(&mut seed)
            .map_err(|_| anyhow::anyhow!("the system random number generator failed"))?;
        Self::from_seed(seed)
    }

    /// The key pair of a 32-byte seed.
    pub fn from_seed(seed: [u8; KEY_LEN]) -> Result<Self> {
        let pair = Ed25519KeyPair::from_seed_unchecked(&seed)
            .map_err(|e| anyhow::anyhow!("not an Ed25519 seed: {e}"))?;
        Ok(Self { seed, pair })
    }

    /// The key pair of a private key in Sparkle's format (base64 of the 32-byte seed). Error
    /// messages never contain the key.
    pub fn from_private_base64(text: &str) -> Result<Self> {
        let bytes = STANDARD
            .decode(text.trim())
            .map_err(|_| anyhow::anyhow!("the private key is not base64"))?;
        let Ok(seed) = <[u8; KEY_LEN]>::try_from(bytes.as_slice()) else {
            bail!(
                "the private key is {} bytes after base64 decoding; expected the {KEY_LEN}-byte seed \
                 that `cargo xtask sparkle keygen` and Sparkle 2's `generate_keys -x` produce",
                bytes.len()
            );
        };
        Self::from_seed(seed)
    }

    /// The private key in Sparkle's format: base64 of the seed. Secret.
    pub fn private_base64(&self) -> String {
        STANDARD.encode(self.seed)
    }

    /// The public key in Sparkle's format (`SUPublicEDKey`): base64 of the 32 bytes.
    pub fn public_base64(&self) -> String {
        STANDARD.encode(self.pair.public_key().as_ref())
    }

    /// The signature of `data` in Sparkle's format (`sparkle:edSignature`).
    pub fn sign_base64(&self, data: &[u8]) -> String {
        STANDARD.encode(self.pair.sign(data).as_ref())
    }
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("public", &self.public_base64())
            .finish_non_exhaustive()
    }
}

/// Checks that `signature` (base64) is the signature of `data` by `public_key` (base64), as an
/// installed copy of the app will.
pub fn verify(public_key: &str, data: &[u8], signature: &str) -> Result<()> {
    let public = STANDARD
        .decode(public_key.trim())
        .context("the public key is not base64")?;
    if public.len() != KEY_LEN {
        bail!("the public key is {} bytes, expected {KEY_LEN}", public.len());
    }
    let signature = STANDARD
        .decode(signature.trim())
        .context("the signature is not base64")?;
    UnparsedPublicKey::new(&ED25519, &public)
        .verify(data, &signature)
        .map_err(|_| anyhow::anyhow!("the signature does not verify with the public key"))
}

/// `sparkle keygen`: the text that hands a new key pair to the user.
pub fn keygen_report(pair: &KeyPair) -> String {
    format!(
        "Generated a new Sparkle EdDSA key pair.\n\
         \n\
         Public key (SUPublicEDKey): put it into `PUBLIC_ED_KEY` in xtask/src/sparkle/keys.rs\n\
         {public}\n\
         \n\
         Private key: store it as the GitHub secret {ENV_PRIVATE_KEY} and in your password\n\
         manager. It is shown only now and is not saved anywhere; never commit it. Losing it\n\
         means installed copies cannot verify new updates.\n\
         {private}\n",
        public = pair.public_base64(),
        private = pair.private_base64(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// RFC 8032, section 7.1, TEST 1 (empty message).
    const RFC_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const RFC_PUBLIC: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const RFC_SIGNATURE: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";

    fn rfc_pair() -> KeyPair {
        let seed = <[u8; KEY_LEN]>::try_from(unhex(RFC_SEED).as_slice()).expect("32 bytes");
        KeyPair::from_seed(seed).expect("pair")
    }

    #[test]
    fn matches_the_rfc_8032_test_vector() {
        let pair = rfc_pair();
        assert_eq!(pair.public_base64(), STANDARD.encode(unhex(RFC_PUBLIC)));
        assert_eq!(pair.sign_base64(b""), STANDARD.encode(unhex(RFC_SIGNATURE)));
        // smoke: the private key round-trips through Sparkle's base64 seed format
        let again = KeyPair::from_private_base64(&pair.private_base64()).expect("round trip");
        assert_eq!(again.public_base64(), pair.public_base64());
        assert_eq!(pair.private_base64(), STANDARD.encode(unhex(RFC_SEED)));
    }

    #[test]
    fn signatures_verify_and_tampering_is_caught() {
        let pair = KeyPair::generate().expect("generate");
        let data = b"pdfxray-0.2.0-macos-universal.dmg bytes";
        let signature = pair.sign_base64(data);

        verify(&pair.public_base64(), data, &signature).expect("verifies");

        let err = verify(&pair.public_base64(), b"other bytes", &signature).expect_err("tampered");
        assert_eq!(
            err.to_string(),
            "the signature does not verify with the public key"
        );
        let other = KeyPair::generate().expect("other");
        assert!(verify(&other.public_base64(), data, &signature).is_err());
        assert!(verify("not base64!", data, &signature).is_err());
        assert!(verify(&STANDARD.encode([1u8; 16]), data, &signature).is_err());
    }

    #[test]
    fn generated_keys_are_random() {
        let a = KeyPair::generate().expect("a");
        let b = KeyPair::generate().expect("b");
        assert_ne!(a.private_base64(), b.private_base64());
        assert_ne!(a.public_base64(), b.public_base64());
        // smoke: both halves are 32 bytes in base64 (44 characters with padding)
        assert_eq!((a.private_base64().len(), a.public_base64().len()), (44, 44));
    }

    #[test]
    fn bad_private_keys_are_rejected_without_echoing_them() {
        let err = KeyPair::from_private_base64("c2VjcmV0LWJ1dC1zaG9ydA==").expect_err("short");
        let text = err.to_string();
        assert!(
            text.contains("16 bytes") && text.contains("32-byte seed"),
            "{text}"
        );
        assert!(!text.contains("c2VjcmV0"), "{text}");

        let err = KeyPair::from_private_base64("#not base64#").expect_err("garbage");
        assert_eq!(err.to_string(), "the private key is not base64");
        // surrounding whitespace (a trailing newline from a secret store) is fine
        let pair = rfc_pair();
        let padded = format!("  {}\n", pair.private_base64());
        assert!(KeyPair::from_private_base64(&padded).is_ok());
    }

    #[test]
    fn debug_output_never_shows_the_private_key() {
        let pair = rfc_pair();
        let debug = format!("{pair:?}");
        assert!(debug.contains(&pair.public_base64()), "{debug}");
        assert!(!debug.contains(&pair.private_base64()), "{debug}");
    }

    #[test]
    fn keygen_report_hands_over_both_halves_with_instructions() {
        let pair = rfc_pair();
        let report = keygen_report(&pair);
        let lines: Vec<&str> = report.lines().collect();
        assert_eq!(lines[3], pair.public_base64());
        assert_eq!(lines.last().copied(), Some(pair.private_base64().as_str()));
        assert!(report.contains("SPARKLE_ED_PRIVATE_KEY") && report.contains("never commit"));
    }

    #[test]
    fn the_committed_public_key_is_a_valid_ed25519_key() {
        let bytes = STANDARD.decode(PUBLIC_ED_KEY).expect("base64");
        assert_eq!(bytes.len(), KEY_LEN);
        // smoke: it is not the RFC test key or all zeros
        assert_ne!(bytes, unhex(RFC_PUBLIC));
        assert_ne!(bytes, vec![0u8; KEY_LEN]);
    }
}
