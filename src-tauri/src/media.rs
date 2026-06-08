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

/// EXIF capture timestamp (Unix secs) — `DateTimeOriginal`, falling back to
/// `DateTimeDigitized` then `DateTime`. Works for JPEG and TIFF-based RAW (NEF).
/// Returns `None` when the file has no EXIF date (the caller then falls back to
/// the filesystem mtime). The EXIF time has no timezone, so it's interpreted as
/// UTC — fine for grouping shots into the month they were taken.
pub fn capture_date(path: &Path) -> Option<i64> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    for tag in [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ] {
        if let Some(field) = exif.get_field(tag, exif::In::PRIMARY) {
            if let exif::Value::Ascii(ref vals) = field.value {
                if let Some(bytes) = vals.first() {
                    if let Some(ts) = parse_exif_datetime(bytes) {
                        return Some(ts);
                    }
                }
            }
        }
    }
    None
}

/// Parse an EXIF datetime string ("YYYY:MM:DD HH:MM:SS") to a Unix timestamp.
fn parse_exif_datetime(b: &[u8]) -> Option<i64> {
    let s = std::str::from_utf8(b).ok()?.trim();
    if s.len() < 19 {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.trim().parse().ok() };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    if year < 1970 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(civil_to_unix(year, month, day, hour, min, sec))
}

/// Convert a civil UTC date-time to a Unix timestamp without pulling in chrono
/// (Howard Hinnant's days-from-civil algorithm). Shared by image EXIF and video
/// `creation_time` parsing.
pub fn civil_to_unix(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // Mar = 0 .. Feb = 11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146097 + doe - 719468; // days since 1970-01-01
    days * 86400 + hh * 3600 + mm * 60 + ss
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

/// Extract the EXIF-embedded **thumbnail** JPEG (the small preview phones/cameras
/// store in the APP1 segment). The main image's SOI sits at offset 0; the
/// thumbnail is a *complete* `FFD8 .. FFD9` JPEG that appears next, near the
/// start of the file. We skip the main SOI and return the first complete JPEG we
/// find — so we can read just the head of a multi-MB file and still get a usable
/// thumbnail without decoding the full-resolution image. Returns `None` if the
/// (truncated) buffer holds no complete embedded JPEG.
pub fn embedded_thumbnail_jpeg(data: &[u8]) -> Option<&[u8]> {
    let mut i = 2usize; // skip the main image's SOI at byte 0
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD8 {
            if let Some(end) = find_eoi(data, i + 2) {
                return Some(&data[i..end]);
            }
        }
        i += 1;
    }
    None
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
