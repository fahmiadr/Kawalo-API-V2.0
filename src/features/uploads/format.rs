//! Recognised file formats and content checks. Nothing the client claims (content type,
//! extension of an image) is trusted: the bytes decide.

/// Images must stay strictly below 150 KB (decimal), the budget the mobile app compresses to.
pub const IMAGE_MAX_BYTES: usize = 150 * 1000;
pub const DOCUMENT_MAX_BYTES: usize = 10 * 1024 * 1024;

const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_MAGIC: [u8; 4] = *b"PK\x03\x04";
/// RIFF header (12 bytes) plus chunk fourcc and size (8 bytes).
const WEBP_HEADER_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Webp,
    Pdf,
    Doc,
    Docx,
    Xls,
    Xlsx,
    Txt,
}

impl Format {
    /// The format a stored file extension stands for (lower case, without the dot).
    pub fn from_ext(ext: &str) -> Option<Self> {
        Some(match ext {
            "webp" => Self::Webp,
            "pdf" => Self::Pdf,
            "doc" => Self::Doc,
            "docx" => Self::Docx,
            "xls" => Self::Xls,
            "xlsx" => Self::Xlsx,
            "txt" => Self::Txt,
            _ => return None,
        })
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Pdf => "pdf",
            Self::Doc => "doc",
            Self::Docx => "docx",
            Self::Xls => "xls",
            Self::Xlsx => "xlsx",
            Self::Txt => "txt",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Webp => "image/webp",
            Self::Pdf => "application/pdf",
            Self::Doc => "application/msword",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xls => "application/vnd.ms-excel",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Txt => "text/plain; charset=utf-8",
        }
    }

    pub fn is_image(self) -> bool {
        self == Self::Webp
    }
}

/// Checks that `data` is a structurally sound WebP: RIFF size consistent with the length and a
/// first chunk that is VP8, VP8L or VP8X and fits in the file, which also rejects truncated files.
pub fn validate_webp(data: &[u8]) -> Result<(), &'static str> {
    const INVALID: &str = "Gambar harus berformat WebP yang valid.";
    if data.len() < WEBP_HEADER_LEN || &data[..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return Err(INVALID);
    }
    let riff_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let chunk_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    let (riff_size, chunk_size) = (u64::from(riff_size), u64::from(chunk_size));
    if riff_size + 8 != data.len() as u64 || WEBP_HEADER_LEN as u64 + chunk_size > data.len() as u64
    {
        return Err(INVALID);
    }
    let payload = &data[WEBP_HEADER_LEN..];
    let sound = match &data[12..16] {
        // Frame tag (3 bytes) then the start code.
        b"VP8 " => chunk_size >= 10 && payload[3..6] == [0x9D, 0x01, 0x2A],
        // Signature byte, then 14+14 bits of size, alpha flag and version.
        b"VP8L" => chunk_size >= 5 && payload[0] == 0x2F,
        b"VP8X" => chunk_size == 10,
        _ => false,
    };
    if sound { Ok(()) } else { Err(INVALID) }
}

/// Checks that `data` really is the document type its extension claims.
pub fn validate_document(format: Format, data: &[u8]) -> Result<(), &'static str> {
    let matches = match format {
        Format::Pdf => data.starts_with(b"%PDF-"),
        Format::Docx | Format::Xlsx => data.starts_with(&ZIP_MAGIC),
        Format::Doc | Format::Xls => data.starts_with(&OLE_MAGIC),
        Format::Txt => std::str::from_utf8(data).is_ok() && !data.contains(&0),
        Format::Webp => false,
    };
    if matches {
        Ok(())
    } else {
        Err("Isi berkas tidak sesuai dengan ekstensinya.")
    }
}

/// A display-only version of the client's file name: no directories, control or bidirectional
/// override characters, at most 255 characters. Never used to build a storage path.
pub fn sanitize_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .take(255)
        .collect();
    cleaned.trim().to_owned()
}

/// Lower-case extension of a sanitized name, if it has a non-empty one.
pub fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn webp(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF".to_vec();
        out.extend(((payload.len() + 12) as u32).to_le_bytes());
        out.extend(b"WEBP");
        out.extend(fourcc);
        out.extend((payload.len() as u32).to_le_bytes());
        out.extend(payload);
        out
    }

    fn vp8l() -> Vec<u8> {
        webp(b"VP8L", &[0x2F, 0, 0, 0, 0, 0])
    }

    #[test]
    fn accepts_each_first_chunk_kind() {
        assert!(validate_webp(&vp8l()).is_ok());
        let vp8 = webp(b"VP8 ", &[0, 0, 0, 0x9D, 0x01, 0x2A, 1, 0, 1, 0]);
        assert!(validate_webp(&vp8).is_ok());
        assert!(validate_webp(&webp(b"VP8X", &[0; 10])).is_ok());
    }

    #[test]
    fn rejects_other_formats_and_garbage() {
        for data in [
            &b""[..],
            b"RIFF",
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0",
            b"\xFF\xD8\xFF\xE0\0\x10JFIF\0\x01\x01\0\0\x01\0\x01\0\0\xFF\xD9",
        ] {
            assert!(validate_webp(data).is_err(), "{data:?}");
        }
    }

    #[test]
    fn rejects_inconsistent_riff_size_and_truncation() {
        let mut trailing = vp8l();
        trailing.push(0);
        assert!(validate_webp(&trailing).is_err());
        let mut truncated = vp8l();
        truncated.truncate(truncated.len() - 2);
        assert!(validate_webp(&truncated).is_err());
        let mut long_chunk = vp8l();
        long_chunk[16] = 200;
        assert!(validate_webp(&long_chunk).is_err());
    }

    #[test]
    fn rejects_unknown_or_malformed_first_chunk() {
        assert!(validate_webp(&webp(b"EXIF", &[0; 10])).is_err());
        assert!(validate_webp(&webp(b"VP8L", &[0x00, 0, 0, 0, 0])).is_err());
        assert!(validate_webp(&webp(b"VP8 ", &[0; 10])).is_err());
        assert!(validate_webp(&webp(b"VP8X", &[0; 9])).is_err());
    }

    #[test]
    fn documents_must_match_their_extension() {
        assert!(validate_document(Format::Pdf, b"%PDF-1.7\n").is_ok());
        assert!(validate_document(Format::Pdf, b"PK\x03\x04").is_err());
        assert!(validate_document(Format::Docx, b"PK\x03\x04rest").is_ok());
        assert!(validate_document(Format::Xlsx, b"%PDF-1.7").is_err());
        assert!(validate_document(Format::Xls, &OLE_MAGIC).is_ok());
        assert!(validate_document(Format::Doc, b"PK\x03\x04").is_err());
        assert!(validate_document(Format::Txt, "halo, dunia \u{2713}".as_bytes()).is_ok());
        assert!(validate_document(Format::Txt, &[0xFF, 0xFE, 0x41]).is_err());
        assert!(validate_document(Format::Txt, b"a\0b").is_err());
    }

    #[test]
    fn names_lose_paths_and_control_characters() {
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name("C:\\tmp\\laporan.pdf"), "laporan.pdf");
        assert_eq!(sanitize_name("a\u{202E}b\0c.txt "), "abc.txt");
        assert_eq!(sanitize_name(&"x".repeat(400)).chars().count(), 255);
        assert_eq!(extension("Laporan.PDF").as_deref(), Some("pdf"));
        assert_eq!(extension(".bashrc"), None);
        assert_eq!(extension("tanpa"), None);
    }
}
