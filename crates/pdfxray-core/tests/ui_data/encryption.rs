//! Encryption.

use pretty_assertions::assert_eq;

use super::*;

/// A file whose `/Encrypt` dictionary is object 6; the trailer `/ID` is the fixed one that the
/// test vectors were computed with.
fn encrypted_with(encrypt: &str) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>");
    w.object(6, encrypt);
    w.finish_table(&Trailer::new(1).encrypt(6)).unwrap();
    Document::from_bytes("enc.pdf", w.into_output().unwrap().bytes).unwrap()
}

const OWNER: &str = "030A11181F262D343B424950575E656C737A81888F969DA4ABB2B9C0C7CED5DC";

#[test]
fn the_encrypted_stub_is_rc4_40_with_the_permissions_of_its_p_entry() {
    let doc = open("encrypted-stub");

    let info = doc.encryption().unwrap();

    assert_eq!(
        (
            info.obj,
            info.filter.as_str(),
            info.sub_filter.as_deref(),
            info.v,
            info.r
        ),
        (Some(r(6)), "Standard", None, Some(1), Some(2))
    );
    assert_eq!(
        (info.algorithm.as_str(), info.length_bits, info.p),
        ("RC4 40-bit", 40, Some(-44))
    );
    // -44 is 0xFFFFFFD4: bits 3 and 5 set (print, copy), bits 4 and 6 clear (modify, annotate).
    // For revision 2 the later flags follow the bit that governed them then.
    let p = info.permissions;
    assert_eq!(
        (
            p.print,
            p.modify,
            p.copy,
            p.annotate,
            p.fill_forms,
            p.extract_accessibility,
            p.assemble,
            p.print_high_quality
        ),
        (true, false, true, false, false, true, false, true)
    );
    assert!(info.metadata_encrypted);
    assert_eq!(
        info.user_password_empty,
        Some(false),
        "the stub's /U is not derived from any password"
    );
    assert!(open("minimal-one-page").encryption().is_none());
    assert!(doc.info().encrypted);
}

#[test]
fn revision_3_reads_the_four_later_permission_bits_and_checks_the_empty_password() {
    // -3900 = 0xFFFFF0C4: only bit 3 (print) of the lower bits; bits 9 to 12 clear.
    let u = "6063CE8713D84CEDE6AE7EF22E8FF5B400000000000000000000000000000000";
    let doc = encrypted_with(&format!(
        "<< /Filter /Standard /V 2 /R 3 /Length 128 /O <{OWNER}> /U <{u}> /P -3900 >>"
    ));

    let info = doc.encryption().unwrap();

    assert_eq!(
        (info.v, info.r, info.length_bits, info.algorithm.as_str()),
        (Some(2), Some(3), 128, "RC4 128-bit")
    );
    let p = info.permissions;
    assert_eq!(
        (
            p.print,
            p.modify,
            p.copy,
            p.annotate,
            p.fill_forms,
            p.extract_accessibility,
            p.assemble,
            p.print_high_quality
        ),
        (true, false, false, false, false, false, false, false)
    );
    assert_eq!(
        info.user_password_empty,
        Some(true),
        "/U is the value the empty password produces"
    );
    assert_eq!(FIXED_ID_HEX.len(), 32);
    let wrong_length = encrypted_with(&format!(
        "<< /Filter /Standard /V 2 /R 3 /Length 40 /O <{OWNER}> /U <{u}> /P -3900 >>"
    ));
    assert_eq!(
        wrong_length.encryption().unwrap().user_password_empty,
        Some(false),
        "a different key length derives a different /U"
    );
}

#[test]
fn a_v4_file_names_its_crypt_filter_and_honours_encrypt_metadata() {
    let u_no_metadata = "F008EAD22680387B263C9D95C5A5DE2300000000000000000000000000000000";
    let u_metadata = "F4B7147222E602E29B6FE7014A7A812C00000000000000000000000000000000";
    let dict = |u: &str, metadata: &str| {
        format!(
            "<< /Filter /Standard /V 4 /R 4 /Length 128 /O <{OWNER}> /U <{u}> /P -1028 {metadata} \
             /CF << /StdCF << /CFM /AESV2 /AuthEvent /DocOpen /Length 16 >> >> /StmF /StdCF /StrF /StdCF >>"
        )
    };

    let off = encrypted_with(&dict(u_no_metadata, "/EncryptMetadata false"))
        .encryption()
        .unwrap();
    let on = encrypted_with(&dict(u_metadata, "")).encryption().unwrap();
    let mismatch = encrypted_with(&dict(u_metadata, "/EncryptMetadata false"))
        .encryption()
        .unwrap();

    assert_eq!(
        (off.v, off.r, off.algorithm.as_str(), off.length_bits),
        (Some(4), Some(4), "AESV2 128-bit", 128)
    );
    assert!(!off.metadata_encrypted && on.metadata_encrypted);
    assert_eq!(off.user_password_empty, Some(true));
    assert_eq!(on.user_password_empty, Some(true));
    assert_eq!(
        mismatch.user_password_empty,
        Some(false),
        "the metadata flag is part of the key"
    );
    // -1028 = 0xFFFFFBFC: print, modify, copy, annotate, fill, extract, high quality; no assembly.
    let p = off.permissions;
    assert_eq!(
        (
            p.print,
            p.modify,
            p.copy,
            p.annotate,
            p.fill_forms,
            p.extract_accessibility,
            p.assemble,
            p.print_high_quality
        ),
        (true, true, true, true, true, true, false, true)
    );
}

#[test]
fn aes_256_and_foreign_handlers_are_summarised_but_the_password_check_is_not_attempted() {
    let aes256 = encrypted_with(&format!(
        "<< /Filter /Standard /V 5 /R 6 /Length 256 /O <{o}> /U <{o}> /OE <{o}> /UE <{o}> /Perms <{o}> /P -1 \
         /CF << /StdCF << /CFM /AESV3 /Length 32 >> >> /StmF /StdCF /StrF /StdCF >>",
        o = OWNER.repeat(3)
    ))
    .encryption()
    .unwrap();
    let foreign = encrypted_with("<< /Filter /Adobe.PubSec /SubFilter /adbe.pkcs7.s5 /V 4 /CF << >> >>")
        .encryption()
        .unwrap();
    let clear = encrypted_with(
        "<< /Filter /Standard /V 4 /R 4 /O <00> /U <00> /P -4 /StmF /Identity /StrF /Identity >>",
    )
    .encryption()
    .unwrap();

    assert_eq!(
        (aes256.algorithm.as_str(), aes256.length_bits, aes256.r),
        ("AESV3 256-bit", 256, Some(6))
    );
    assert_eq!(
        aes256.user_password_empty, None,
        "revision 6 needs SHA-2 and AES, which pdfxray does not carry"
    );
    assert_eq!(
        (foreign.filter.as_str(), foreign.sub_filter.as_deref()),
        ("Adobe.PubSec", Some("adbe.pkcs7.s5"))
    );
    assert_eq!(foreign.user_password_empty, None);
    assert_eq!(clear.algorithm, "None");
    assert_eq!(aes256.p, Some(-1));
    assert!(aes256.permissions.print && aes256.permissions.assemble);
}
