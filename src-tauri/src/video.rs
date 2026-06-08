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
