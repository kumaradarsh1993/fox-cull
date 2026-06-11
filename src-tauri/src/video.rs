//! Video poster frames (and, in Stage 2, lossless trim) via a **bundled ffmpeg**.
//!
//! ffmpeg ships as a Tauri `externalBin`, so at runtime it sits next to our own
//! executable. We invoke it directly with `std::process::Command` (no shell
//! plugin, no extra capability) and cache a single decoded frame as a JPEG in the
//! same on-disk thumbnail cache as images — which, by following the catalog onto
//! the user's SSD, means a clip's poster is generated once and reused on every
//! machine that reads that SSD.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

/// Resolve the bundled ffmpeg sitting beside our executable (Tauri strips the
/// target-triple suffix from the externalBin at bundle time). `None` if absent
/// (e.g. a dev run without the binary) — callers then fall back to a placeholder.
pub fn ffmpeg_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let p = dir.join(name);
    p.exists().then_some(p)
}

fn meta(src: &Path) -> (i64, u64) {
    match std::fs::metadata(src) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            (mtime, m.len())
        }
        Err(_) => (0, 0),
    }
}

/// Cache path for a clip's poster, keyed by (path, mtime, size) and prefixed `v`
/// so it never collides with image thumbnails.
pub fn poster_path(cache_dir: &Path, src: &Path) -> PathBuf {
    let (mtime, size) = meta(src);
    let abs = src.to_string_lossy();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    mtime.hash(&mut h);
    size.hash(&mut h);
    cache_dir.join(format!("v{:016x}.jpg", h.finish()))
}

/// Extract one representative frame (~1s in) scaled to fit a 480px box and write
/// it to `out`. Idempotent. Works for any codec the bundled ffmpeg supports —
/// crucially including HEVC (the Osmo Pocket 3 footage the webview can't decode).
pub fn make_poster(ffmpeg: &Path, src: &Path, out: &Path) -> Result<(), String> {
    if out.exists() {
        return Ok(());
    }
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-ss", "1", "-i"])
        .arg(src)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=w=480:h=480:force_original_aspect_ratio=decrease",
            "-y",
        ])
        .arg(out)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Don't flash a console window on Windows for each clip.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() && out.exists() {
        Ok(())
    } else {
        // Some clips are shorter than the seek point; retry from the very start.
        let mut cmd2 = Command::new(ffmpeg);
        cmd2.args(["-v", "error", "-i"])
            .arg(src)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=w=480:h=480:force_original_aspect_ratio=decrease",
                "-y",
            ])
            .arg(out)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd2.creation_flags(CREATE_NO_WINDOW);
        }
        let s2 = cmd2.status().map_err(|e| e.to_string())?;
        if s2.success() && out.exists() {
            Ok(())
        } else {
            Err("ffmpeg could not extract a poster frame".into())
        }
    }
}

/// Lossless trim: copy the stream between `in_s` and `out_s` (seconds) to `dest`
/// with NO re-encode (`-c copy`) — instant even on huge files, exactly like
/// LosslessCut. `-ss` before `-i` does a fast keyframe seek; `-t` gives the
/// duration so the cut length is unambiguous. Returns the output path.
pub fn trim(
    ffmpeg: &Path,
    src: &Path,
    in_s: f64,
    out_s: f64,
    dest: &Path,
) -> Result<(), String> {
    if out_s <= in_s {
        return Err("out point must be after in point".into());
    }
    let dur = out_s - in_s;
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-ss", &format!("{in_s:.3}"), "-i"])
        .arg(src)
        .args([
            "-t",
            &format!("{dur:.3}"),
            "-c",
            "copy",
            "-map",
            "0",
            "-avoid_negative_ts",
            "make_zero",
            "-y",
        ])
        .arg(dest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() && dest.exists() {
        Ok(())
    } else {
        Err("ffmpeg trim failed".into())
    }
}

/// Decode a still image file (e.g. HEIC, which neither the webview nor the
/// `image` crate can read) into a JPEG at `out`, scaled to fit a `max` box.
/// ffmpeg's HEIF demuxer applies the container's rotation (irot/imir), so the
/// output is upright without us reading EXIF.
pub fn decode_still(ffmpeg: &Path, src: &Path, out: &Path, max: u32) -> Result<(), String> {
    let vf = format!("scale=w={max}:h={max}:force_original_aspect_ratio=decrease");
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-i"])
        .arg(src)
        .args(["-frames:v", "1", "-vf", &vf, "-q:v", "3", "-y"])
        .arg(out)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() && out.exists() {
        Ok(())
    } else {
        Err("ffmpeg could not decode this image".into())
    }
}

// ── H.264 proxy playback (HEVC clips on machines without the OS codec) ───────
//
// The webview plays video through the OS media stack (Media Foundation on
// Windows), so we can't bundle a codec INTO the player — but the bundled ffmpeg
// decodes HEVC fine. When a clip genuinely fails to play, we transcode a capped
// H.264 preview once, cache it beside the thumbnails on the drive, and play
// that. One transcode at a time (a second concurrent one would just thrash the
// disk and halve both).

/// Cache path for a clip's H.264 proxy, keyed like the poster but prefixed `p`.
pub fn proxy_path(cache_dir: &Path, src: &Path) -> PathBuf {
    let (mtime, size) = meta(src);
    let abs = src.to_string_lossy();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    mtime.hash(&mut h);
    size.hash(&mut h);
    cache_dir.join(format!("p{:016x}.mp4", h.finish()))
}

static PROXY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Transcode `src` to a capped (≤1920 long edge) H.264/AAC preview at `out`,
/// reporting progress (0.0..=1.0) via `on_progress`. Writes to a `.part` file
/// and renames on success so a crash never leaves a half-written proxy that
/// would later "play" as truncated. Serialized process-wide.
pub fn ensure_proxy(
    cache_dir: &Path,
    ffmpeg: Option<&Path>,
    src: &Path,
    mut on_progress: impl FnMut(f64),
) -> Result<PathBuf, String> {
    let out = proxy_path(cache_dir, src);
    if out.exists() {
        return Ok(out);
    }
    let ff = ffmpeg.ok_or("ffmpeg not available")?;
    let _guard = PROXY_LOCK.lock().map_err(|_| "proxy lock poisoned")?;
    if out.exists() {
        return Ok(out); // built while we waited for the lock
    }
    let dur = probe_duration(ff, src).unwrap_or(0.0);
    let part = out.with_extension("part.mp4");
    let _ = std::fs::remove_file(&part);

    let mut cmd = Command::new(ff);
    cmd.args(["-v", "error", "-i"])
        .arg(src)
        .args([
            "-map", "0:v:0", "-map", "0:a:0?",
            "-vf",
            "scale=w=1920:h=1920:force_original_aspect_ratio=decrease:force_divisible_by=2",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "22",
            "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "128k",
            "-movflags", "+faststart",
            "-progress", "pipe:1", "-nostats",
            "-y",
        ])
        .arg(&part)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(stdout) = child.stdout.take() {
        use std::io::BufRead;
        // ffmpeg's -progress stream is `key=value` lines; out_time_us tracks the
        // encoded position, which against the probed duration gives a fraction.
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if dur > 0.0 {
                if let Some(v) = line.strip_prefix("out_time_us=") {
                    if let Ok(us) = v.trim().parse::<f64>() {
                        on_progress((us / 1_000_000.0 / dur).clamp(0.0, 1.0));
                    }
                }
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() && part.exists() {
        std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
        on_progress(1.0);
        Ok(out)
    } else {
        let _ = std::fs::remove_file(&part);
        Err("ffmpeg could not convert this clip (the build may lack an H.264 encoder)".into())
    }
}

/// Ensure a poster exists for `src`; returns its cache path. `ffmpeg=None` (not
/// bundled / dev) yields an error so the UI shows the film placeholder.
pub fn ensure_poster(cache_dir: &Path, ffmpeg: Option<&Path>, src: &Path) -> Result<PathBuf, String> {
    let out = poster_path(cache_dir, src);
    if out.exists() {
        return Ok(out);
    }
    let ff = ffmpeg.ok_or("ffmpeg not available")?;
    make_poster(ff, src, &out)?;
    Ok(out)
}

// ── filmstrip scrub (Tier 2: hover-preview + seek without webview decode) ─────
//
// We pre-extract a fixed grid of frames spread across the clip into ONE sprite
// JPEG (the "filmstrip"), cached on the SSD beside the poster. The webview then
// shows the frame under the scrub cursor instantly by offsetting a CSS sprite —
// no per-frame decode in the player, so scrubbing is smooth even on HEVC the
// webview can't natively decode (the Osmo Pocket 3 footage). Generated lazily on
// first loupe open, then reused on every machine that reads the SSD.

const FILMSTRIP_COLS: u32 = 10;
/// Pixel width each frame is scaled to inside the sprite (height follows aspect,
/// so portrait phone clips stay portrait).
const FILMSTRIP_TILE_W: u32 = 160;

/// Geometry of a generated filmstrip, persisted in a tiny sidecar so we don't
/// re-probe the clip on every loupe open.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Filmstrip {
    pub cols: u32,
    pub rows: u32,
    /// Number of real frames in the sprite (<= cols*rows; trailing cells blank).
    pub count: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    /// Clip duration in seconds — lets the frontend map cursor → time → frame.
    pub duration: f64,
}

/// Sprite-sheet cache path, prefixed `f` so it never collides with image
/// thumbnails (`<hash>`) or posters (`v<hash>`).
pub fn filmstrip_path(cache_dir: &Path, src: &Path) -> PathBuf {
    let (mtime, size) = meta(src);
    let abs = src.to_string_lossy();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    abs.hash(&mut h);
    mtime.hash(&mut h);
    size.hash(&mut h);
    cache_dir.join(format!("f{:016x}.jpg", h.finish()))
}

/// Read the clip's duration (seconds) from ffmpeg's stderr banner. ffmpeg with
/// only `-i` (no output) exits non-zero but still prints `Duration: HH:MM:SS.xx`,
/// so we parse that — no separate ffprobe binary needed.
fn probe_duration(ffmpeg: &Path, src: &Path) -> Option<f64> {
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-i")
        .arg(src)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    let err = String::from_utf8_lossy(&out.stderr);
    let idx = err.find("Duration:")?;
    let token = err[idx + "Duration:".len()..]
        .trim_start()
        .split(',')
        .next()?
        .trim();
    if token.starts_with("N/A") {
        return None;
    }
    let mut parts = token.split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let s: f64 = parts.next()?.trim().parse().ok()?;
    let secs = h * 3600.0 + m * 60.0 + s;
    (secs > 0.0).then_some(secs)
}

/// Capture time (Unix secs) of a clip from its `creation_time` metadata, parsed
/// from ffmpeg's `-i` banner. ISO-8601 UTC ("2024-05-01T12:34:56.000000Z").
/// `None` when absent (e.g. re-encoded clips) so the caller falls back to mtime.
pub fn creation_time(ffmpeg: &Path, src: &Path) -> Option<i64> {
    let mut cmd = Command::new(ffmpeg);
    cmd.arg("-i")
        .arg(src)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    let err = String::from_utf8_lossy(&out.stderr);
    let idx = err.find("creation_time")?;
    // After the key: "creation_time   : 2024-05-01T12:34:56.000000Z"
    let after = &err[idx..];
    let colon = after.find(':')?;
    let val = after[colon + 1..].trim_start();
    let token = val.split_whitespace().next()?;
    parse_iso(token)
}

/// Parse "YYYY-MM-DDThh:mm:ss…" (ignoring sub-seconds / trailing Z) to Unix secs.
fn parse_iso(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse().ok() };
    let y = num(0, 4)?;
    let mo = num(5, 7)?;
    let d = num(8, 10)?;
    let h = num(11, 13)?;
    let mi = num(14, 16)?;
    let se = num(17, 19)?;
    if y < 1970 || !(1..=12).contains(&mo) {
        return None;
    }
    Some(crate::media::civil_to_unix(y, mo, d, h, mi, se))
}

/// Render the sprite: `count` frames evenly spread across the clip (`fps` filter),
/// each scaled to FILMSTRIP_TILE_W wide, tiled into a `cols x rows` grid. The
/// `tile` filter flushes a partial last frame at EOF, so short clips still yield
/// one image (trailing cells blank — never addressed by the frontend).
fn make_filmstrip(
    ffmpeg: &Path,
    src: &Path,
    out: &Path,
    cols: u32,
    rows: u32,
    fps: f64,
) -> Result<(), String> {
    let vf = format!(
        "fps={fps:.6},scale={FILMSTRIP_TILE_W}:-2,tile={cols}x{rows}:padding=0:margin=0"
    );
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-i"])
        .arg(src)
        .args(["-vf", &vf, "-frames:v", "1", "-an", "-y"])
        .arg(out)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() && out.exists() {
        Ok(())
    } else {
        Err("ffmpeg could not build filmstrip".into())
    }
}

/// Ensure a filmstrip sprite + its geometry sidecar exist for `src`; returns the
/// sprite path and geometry. Cached (sprite `f<hash>.jpg` + `f<hash>.json`).
pub fn ensure_filmstrip(
    cache_dir: &Path,
    ffmpeg: Option<&Path>,
    src: &Path,
) -> Result<(PathBuf, Filmstrip), String> {
    let sprite = filmstrip_path(cache_dir, src);
    let meta_path = sprite.with_extension("json");
    if sprite.exists() && meta_path.exists() {
        if let Ok(txt) = std::fs::read_to_string(&meta_path) {
            if let Ok(fs) = serde_json::from_str::<Filmstrip>(&txt) {
                return Ok((sprite, fs));
            }
        }
    }
    let ff = ffmpeg.ok_or("ffmpeg not available")?;
    let duration = probe_duration(ff, src).ok_or("could not read video duration")?;
    // ~1 frame/second, clamped 16..=100, so short clips stay dense and long ones
    // don't blow up the sprite. cols fixed at 10; rows = ceil(count/cols).
    let count = (duration.round() as u32).clamp(16, 100);
    let cols = FILMSTRIP_COLS;
    let rows = count.div_ceil(cols);
    let fps = count as f64 / duration;
    make_filmstrip(ff, src, &sprite, cols, rows, fps)?;
    let (w, h) = image::image_dimensions(&sprite).map_err(|e| e.to_string())?;
    let fs = Filmstrip {
        cols,
        rows,
        count,
        tile_w: (w / cols).max(1),
        tile_h: (h / rows).max(1),
        duration,
    };
    if let Ok(txt) = serde_json::to_string(&fs) {
        let _ = std::fs::write(&meta_path, txt);
    }
    Ok((sprite, fs))
}
