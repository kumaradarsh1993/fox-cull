//! Orientation-baked, disk-cached thumbnails (and larger loupe previews).
//!
//! Cache filename is a hash of (abs path, mtime, size, max-edge, orientation),
//! so editing/replacing a file or asking for a different size produces a fresh
//! entry and stale ones are simply never referenced again.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use image::DynamicImage;

use crate::media::{self, Kind};

/// The cache file path for `path` at long-edge `max` (whether or not it exists
/// yet). Used both by `ensure` and by cleanup-on-delete to find the exact file
/// to remove. Reads the file's metadata, so call it while the file still exists.
pub fn cache_path(cache_dir: &Path, path: &Path, max: u32) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let abs = path.to_string_lossy().to_string();
    let o = media::orientation(path);
    Some(cache_dir.join(format!("{}.jpg", cache_key(&abs, mtime, meta.len(), max, o))))
}

fn cache_key(abs: &str, mtime: i64, size: u64, max: u32, orientation: u16) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    mtime.hash(&mut h);
    size.hash(&mut h);
    max.hash(&mut h);
    orientation.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Bake the EXIF orientation into the pixels so the output is always upright —
/// this is the fix for "portrait phone photos showing up sideways".
fn apply_orientation(img: DynamicImage, o: u16) -> DynamicImage {
    match o {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// DCT-scaled JPEG decode: ask the decoder for the smallest 1/1·1/2·1/4·1/8
/// scale that still covers `max`, so a 50MP phone JPEG targeted at a 320px
/// thumbnail does ~1/64th of the IDCT work. Returns `None` for color spaces we
/// don't fast-path (CMYK/16-bit) or on any decode hiccup, so the caller falls
/// back to the full `image` decoder for correctness.
fn decode_jpeg_scaled(bytes: &[u8], max: u32) -> Option<DynamicImage> {
    use jpeg_decoder::{Decoder, PixelFormat};
    let mut dec = Decoder::new(std::io::Cursor::new(bytes));
    dec.scale(max as u16, max as u16).ok()?;
    let pixels = dec.decode().ok()?;
    let info = dec.info()?;
    let (w, h) = (info.width as u32, info.height as u32);
    match info.pixel_format {
        PixelFormat::RGB24 => image::RgbImage::from_raw(w, h, pixels).map(DynamicImage::ImageRgb8),
        PixelFormat::L8 => image::GrayImage::from_raw(w, h, pixels).map(DynamicImage::ImageLuma8),
        // CMYK32 / L16 are rare (not phone photos) — let the caller full-decode.
        _ => None,
    }
}

/// Fast path for grid thumbnails: decode ONLY the EXIF-embedded thumbnail that
/// phones/cameras store in the file header. Reads just the first ~768 KB (not the
/// whole multi-MB file) and decodes a ~512px image, so this is ~7x less disk I/O
/// and a near-instant decode. Returns `None` if there's no embedded thumbnail or
/// it's too small to look crisp at `max` — the caller then full-decodes.
fn embedded_thumb(path: &Path, max: u32) -> Option<DynamicImage> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(768 * 1024);
    f.take(768 * 1024).read_to_end(&mut buf).ok()?;
    let jpg = media::embedded_thumbnail_jpeg(&buf)?;
    let img = image::load_from_memory(jpg).ok()?;
    // Accept only if the embedded thumb fills at least 75% of the target edge,
    // otherwise it'd look soft (some cameras embed a tiny 160px thumbnail).
    let long = img.width().max(img.height());
    if long * 4 >= max * 3 {
        Some(img)
    } else {
        None
    }
}

fn load_source(path: &Path, kind: Kind, max: u32) -> Result<DynamicImage, String> {
    match kind {
        // RAW: pull the embedded full-res JPEG preview rather than demosaic,
        // and DCT-scale that preview too.
        Kind::Raw => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let jpg = media::largest_embedded_jpeg(&data)
                .ok_or_else(|| "no embedded JPEG preview found in RAW file".to_string())?;
            if let Some(img) = decode_jpeg_scaled(jpg, max) {
                return Ok(img);
            }
            image::load_from_memory(jpg).map_err(|e| e.to_string())
        }
        _ => {
            // Fast path for the dominant case (95% of the user's library is
            // phone JPEG): read once, DCT-scaled decode from memory.
            if matches!(media::ext_lower(path).as_str(), "jpg" | "jpeg") {
                if let Ok(bytes) = std::fs::read(path) {
                    if let Some(img) = decode_jpeg_scaled(&bytes, max) {
                        return Ok(img);
                    }
                }
            }
            image::open(path).map_err(|e| e.to_string())
        }
    }
}

/// Ensure a cached, orientation-corrected JPEG (long edge <= `max`) exists for
/// `path`; returns the cache file path. Idempotent — returns immediately if the
/// keyed file already exists.
pub fn ensure(cache_dir: &Path, path: &Path, kind: Kind, max: u32) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let abs = path.to_string_lossy().to_string();
    let o = media::orientation(path);
    let out = cache_dir.join(format!("{}.jpg", cache_key(&abs, mtime, meta.len(), max, o)));
    if out.exists() {
        return Ok(out);
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let t0 = Instant::now();
    // Grid-sized JPEG thumbnails: try the embedded-thumbnail fast path first.
    let mut source = "full";
    let img = if max <= 512
        && matches!(kind, Kind::Image)
        && matches!(media::ext_lower(path).as_str(), "jpg" | "jpeg")
    {
        match embedded_thumb(path, max) {
            Some(i) => {
                source = "embed";
                i
            }
            None => load_source(path, kind, max)?,
        }
    } else {
        load_source(path, kind, max)?
    };
    let (sw, sh) = (img.width(), img.height());
    let decode_ms = t0.elapsed().as_millis();
    let t1 = Instant::now();
    // Downscale FIRST, then bake orientation into the small image — rotating a
    // full 12MP buffer before downscaling was a big chunk of the resize cost.
    let img = img.thumbnail(max, max); // preserves aspect, fits within max x max
    let img = apply_orientation(img, o);
    // JPEG has no alpha channel; flatten to RGB before encoding.
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    rgb.save(&out).map_err(|e| e.to_string())?;
    crate::log::line(&format!(
        "THUMB-GEN {} {}x{} max={} src={} decode={}ms resize+enc={}ms",
        name,
        sw,
        sh,
        max,
        source,
        decode_ms,
        t1.elapsed().as_millis()
    ));
    Ok(out)
}
