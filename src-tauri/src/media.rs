//! Media classification, EXIF orientation, and embedded-preview extraction.
//!
//! The speed trick for culling is *don't decode, extract*: for JPEGs the
//! browser/`image` crate reads them directly, and for Nikon `.NEF` (and other
//! RAW) files we pull the full-resolution JPEG that the camera embedded inside
//! the file rather than demosaicing the raw sensor data.

use std::path::Path;

/// Coarse kind, surfaced to the frontend so it can pick a render path.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Raw,
    Video,
    Other,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Raw => "raw",
            Kind::Video => "video",
            Kind::Other => "other",
        }
    }
}

pub fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn classify(path: &Path) -> Kind {
    match ext_lower(path).as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" | "gif" => Kind::Image,
        "nef" | "dng" | "cr2" | "cr3" | "arw" | "raf" | "rw2" | "orf" | "srw" | "pef" => Kind::Raw,
        "mp4" | "mov" | "m4v" | "avi" | "mkv" | "webm" | "3gp" => Kind::Video,
        _ => Kind::Other,
    }
}

pub fn is_media(path: &Path) -> bool {
    !matches!(classify(path), Kind::Other)
}

/// EXIF orientation tag (1..=8). Defaults to 1 (normal) when absent/unreadable.
/// Works for JPEG and TIFF-based RAW (NEF) via the kamadak-exif container reader.
pub fn orientation(path: &Path) -> u16 {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };
    let mut reader = std::io::BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(_) => return 1,
    };
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .map(|v| v as u16)
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1)
}

/// Scan a byte buffer for the largest embedded JPEG (`FFD8 .. FFD9`). RAW files
/// embed a small thumbnail *and* a large preview as separate JPEG streams; the
/// largest is the full-resolution preview we want for culling.
pub fn largest_embedded_jpeg(data: &[u8]) -> Option<&[u8]> {
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0usize;
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD8 {
            if let Some(end) = find_eoi(data, i + 2) {
                let len = end - i;
                let better = best.map_or(true, |(s, e)| len > (e - s));
                if better {
                    best = Some((i, end));
                }
                i = end; // skip past this stream
                continue;
            }
        }
        i += 1;
    }
    best.map(|(s, e)| &data[s..e])
}

/// Index just past the `FFD9` end-of-image marker, searching from `start`.
fn find_eoi(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD9 {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}
