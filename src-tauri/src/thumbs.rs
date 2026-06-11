//! Orientation-baked, disk-cached thumbnails (and larger loupe previews).
//!
//! Cache filename is a hash of (abs path, mtime, size, max-edge), so editing or
//! replacing a file (mtime/size change) or asking for a different size produces a
//! fresh entry and stale ones are simply never referenced again.
//!
//! CRITICAL: orientation is deliberately NOT in the key. Orientation is a property
//! of the file's bytes, so it cannot change unless mtime/size already does — adding
//! it to the key bought nothing, but reading it (which OPENS the original file to
//! parse its EXIF header) on every cache *lookup* turned a cheap of cache hits into
//! a seek storm against the originals. On a spinning disk, revisiting a folder of
//! already-cached thumbnails meant hundreds of original-file opens just to rebuild
//! keys — the "(Not Responding) that recovers if you wait" freeze. We now read
//! orientation ONLY when actually generating a missing thumbnail.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use image::DynamicImage;

use crate::media::{self, Kind};

/// The cache file path for `path` at long-edge `max` (whether or not it exists
/// yet). Used both by `ensure` and by cleanup-on-delete to find the exact file
/// to remove. Reads the file's metadata (a cheap stat), so call it while the file
/// still exists — but never opens the file, so it stays fast on a spinning disk.
pub fn cache_path(cache_dir: &Path, path: &Path, max: u32) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let abs = path.to_string_lossy().to_string();
    Some(cache_dir.join(format!("{}.jpg", cache_key(&abs, mtime, meta.len(), max))))
}

fn cache_key(abs: &str, mtime: i64, size: u64, max: u32) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    mtime.hash(&mut h);
    size.hash(&mut h);
    max.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Bake the EXIF orientation into the pixels so the output is always upright —
/// this is the fix for "portrait phone photos showing up sideways". Also used
/// by the JPEG export (the RAW embedded preview carries no orientation tag).
pub fn apply_orientation(img: DynamicImage, o: u16) -> DynamicImage {
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
/// and a near-instant decode. Also returns the file's ICC profile (the APP2
/// segments sit in the same head bytes) so the cached thumbnail keeps its colors.
/// Returns `None` if there's no embedded thumbnail or it's too small to look
/// crisp at `max` — the caller then full-decodes.
fn embedded_thumb(path: &Path, max: u32) -> Option<(DynamicImage, Option<Vec<u8>>)> {
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
        let icc = media::icc_from_jpeg(&buf);
        Some((img, icc))
    } else {
        None
    }
}

/// Decoded source image plus its ICC color profile (when the source carries
/// one), so re-encoded thumbnails/previews keep wide-gamut colors instead of
/// silently being reinterpreted as sRGB.
fn load_source(path: &Path, kind: Kind, max: u32) -> Result<(DynamicImage, Option<Vec<u8>>), String> {
    match kind {
        // RAW: pull the embedded full-res JPEG preview rather than demosaic,
        // and DCT-scale that preview too. The profile (sRGB or — if the camera
        // was set to AdobeRGB — that) rides along in the preview's APP2.
        Kind::Raw => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let jpg = media::largest_embedded_jpeg(&data)
                .ok_or_else(|| "no embedded JPEG preview found in RAW file".to_string())?;
            let icc = media::icc_from_jpeg(jpg);
            if let Some(img) = decode_jpeg_scaled(jpg, max) {
                return Ok((img, icc));
            }
            image::load_from_memory(jpg)
                .map(|i| (i, icc))
                .map_err(|e| e.to_string())
        }
        _ => {
            // Fast path for the dominant case (95% of the user's library is
            // phone JPEG): read once, DCT-scaled decode from memory.
            if matches!(media::ext_lower(path).as_str(), "jpg" | "jpeg") {
                if let Ok(bytes) = std::fs::read(path) {
                    let icc = media::icc_from_jpeg(&bytes);
                    if let Some(img) = decode_jpeg_scaled(&bytes, max) {
                        return Ok((img, icc));
                    }
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        return Ok((img, icc));
                    }
                }
            }
            image::open(path).map(|i| (i, None)).map_err(|e| e.to_string())
        }
    }
}

/// Splice an ICC profile into an existing JPEG file as APP2 `ICC_PROFILE`
/// segment(s), inserted after any APP0/APP1 headers. The `image` crate's
/// encoder writes untagged JPEGs, which the webview renders as sRGB — for a
/// Display P3 / AdobeRGB source that visibly mutes the colors. This puts the
/// source's profile back so the browser color-manages the preview correctly.
pub fn embed_icc(out: &Path, icc: &[u8]) -> std::io::Result<()> {
    const SIG: &[u8] = b"ICC_PROFILE\0";
    const MAX_DATA: usize = 65533 - SIG.len() - 2; // segment length field max minus headers
    let data = std::fs::read(out)?;
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Ok(()); // not a JPEG we understand — leave it untouched
    }
    // Insert after the APP0/APP1 run (JFIF/EXIF first, per convention).
    let mut pos = 2usize;
    while pos + 4 <= data.len()
        && data[pos] == 0xFF
        && (data[pos + 1] == 0xE0 || data[pos + 1] == 0xE1)
    {
        let len = ((data[pos + 2] as usize) << 8) | data[pos + 3] as usize;
        if len < 2 || pos + 2 + len > data.len() {
            return Ok(());
        }
        pos += 2 + len;
    }
    let chunks: Vec<&[u8]> = icc.chunks(MAX_DATA).collect();
    let count = chunks.len().min(255) as u8;
    let mut seg = Vec::with_capacity(icc.len() + 32 * chunks.len());
    for (idx, chunk) in chunks.iter().enumerate().take(255) {
        let len = 2 + SIG.len() + 2 + chunk.len();
        seg.push(0xFF);
        seg.push(0xE2);
        seg.push((len >> 8) as u8);
        seg.push((len & 0xFF) as u8);
        seg.extend_from_slice(SIG);
        seg.push(idx as u8 + 1);
        seg.push(count);
        seg.extend_from_slice(chunk);
    }
    let mut rebuilt = Vec::with_capacity(data.len() + seg.len());
    rebuilt.extend_from_slice(&data[..pos]);
    rebuilt.extend_from_slice(&seg);
    rebuilt.extend_from_slice(&data[pos..]);
    std::fs::write(out, rebuilt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: encode a JPEG, splice a profile in, read it back out, and
    /// confirm the image still decodes.
    #[test]
    fn icc_embed_roundtrip() {
        let dir = std::env::temp_dir().join("foxcull-icc-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("t.jpg");
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            8,
            image::Rgb([200, 90, 30]),
        ));
        img.save(&out).unwrap();
        let profile: Vec<u8> = (0u8..255).cycle().take(200_000).collect(); // forces multi-chunk
        embed_icc(&out, &profile).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let got = crate::media::icc_from_jpeg(&bytes).expect("profile back out");
        assert_eq!(got, profile);
        image::load_from_memory(&bytes).expect("JPEG still decodes after splice");
        let _ = std::fs::remove_file(&out);
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
    let out = cache_dir.join(format!("{}.jpg", cache_key(&abs, mtime, meta.len(), max)));
    if out.exists() {
        return Ok(out);
    }
    // HEIC/HEIF: the webview and the `image` crate can't decode these — hand
    // the whole job (decode + container rotation + scale + JPEG encode) to the
    // bundled ffmpeg. ICC is not carried over (ffmpeg writes an untagged JPEG).
    if matches!(media::ext_lower(path).as_str(), "heic" | "heif") {
        let ff = crate::video::ffmpeg_path()
            .ok_or_else(|| "HEIC preview needs the bundled ffmpeg".to_string())?;
        crate::video::decode_still(&ff, path, &out, max)?;
        return Ok(out);
    }
    // Only now — for a thumbnail we actually have to build — do we open the
    // original to read its EXIF orientation (so it gets baked into the pixels).
    let o = media::orientation(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let t0 = Instant::now();
    // Grid-sized JPEG thumbnails: try the embedded-thumbnail fast path first.
    let mut source = "full";
    let (img, icc) = if max <= 512
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
    // Re-attach the source's color profile (wide-gamut phone shots / AdobeRGB
    // camera previews) so the cached JPEG renders with the right colors.
    if let Some(icc) = icc {
        let _ = embed_icc(&out, &icc);
    }
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
