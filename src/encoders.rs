//! Encoder availability detection and hardware-encoder resolution.
//!
//! Two stages, both driven by the `ffmpeg` executable:
//!
//! 1. `ffmpeg -encoders` tells us which encoders are *compiled in*.
//! 2. Each hardware candidate that survived stage 1 is exercised with a
//!    one-frame `-f null` encode. A compiled-in `h264_nvenc` on a machine with
//!    no NVIDIA GPU lists fine but fails at runtime, so stage 1 alone is not
//!    trustworthy.
//!
//! Detection runs on a background thread (it costs a couple of seconds) and the
//! result is cached in the config directory, so the UI always has *something*
//! immediately and never blocks waiting for a subprocess.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::probe::ffmpeg_bin;

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncoderRegistry {
    /// First line of `ffmpeg -version` (cache key).
    pub version: String,
    pub video: BTreeSet<String>,
    pub audio: BTreeSet<String>,
    /// Hardware encoders that were actually exercised, `true` = it produced a
    /// frame. Only probed encoders appear here.
    pub hw: BTreeMap<String, bool>,
    /// Set when `ffmpeg -encoders` itself could not be run.
    pub error: Option<String>,
    /// Unix seconds of the detection run.
    pub detected_at: i64,
}

impl EncoderRegistry {
    /// Hardware encoders that were verified end-to-end.
    pub fn working_hardware(&self) -> Vec<&str> {
        self.hw
            .iter()
            .filter(|(_, ok)| **ok)
            .map(|(n, _)| n.as_str())
            .collect()
    }

    pub fn is_usable(&self, name: &str) -> bool {
        match self.hw.get(name) {
            Some(ok) => *ok,
            None => self.video.contains(name) || self.audio.contains(name),
        }
    }

    /// e.g. `"142 video · 48 audio · hw: h264_nvenc, hevc_nvenc"`
    pub fn summary(&self) -> String {
        if let Some(err) = &self.error {
            return err.clone();
        }
        let hw = self.working_hardware();
        format!(
            "{} video · {} audio · hw: {}",
            self.video.len(),
            self.audio.len(),
            if hw.is_empty() {
                "none".to_string()
            } else {
                hw.join(", ")
            }
        )
    }
}

/// Parse the table printed by `ffmpeg -encoders`.
///
/// Lines look like ` V....D libx264   H.264 / AVC ... (codec h264)`, preceded by
/// a `------` separator that we use to skip the legend.
pub fn parse_encoders(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut video = BTreeSet::new();
    let mut audio = BTreeSet::new();
    let mut in_table = false;
    for line in text.lines() {
        if !in_table {
            if line.trim_start().starts_with("------") {
                in_table = true;
            }
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(flags), Some(name)) = (it.next(), it.next()) else {
            continue;
        };
        if flags.len() != 6 {
            continue;
        }
        match flags.as_bytes()[0] {
            b'V' => {
                video.insert(name.to_string());
            }
            b'A' => {
                audio.insert(name.to_string());
            }
            _ => {}
        }
    }
    (video, audio)
}

// ---------------------------------------------------------------------------
// Hardware encoder knowledge
// ---------------------------------------------------------------------------

/// Encoders we probe with a real encode. Ordered by general preference.
pub const HW_CANDIDATES: &[&str] = &[
    "h264_nvenc",
    "hevc_nvenc",
    "av1_nvenc",
    "h264_qsv",
    "hevc_qsv",
    "av1_qsv",
    "vp9_qsv",
    "h264_amf",
    "hevc_amf",
    "av1_amf",
    "h264_mf",
    "hevc_mf",
    "h264_videotoolbox",
    "hevc_videotoolbox",
    "h264_vaapi",
    "hevc_vaapi",
];

/// Software codec → hardware encoders that produce the same codec.
///
/// `*_vaapi` is deliberately absent: it needs `format=nv12,hwupload` in the
/// filter graph plus a `-vaapi_device` input option, i.e. a different pipeline.
/// It stays selectable by hand but is never auto-picked.
pub const HW_PREFERENCE: &[(&str, &[&str])] = &[
    (
        "libx264",
        &["h264_nvenc", "h264_qsv", "h264_amf", "h264_mf", "h264_videotoolbox"],
    ),
    (
        "libx265",
        &["hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_mf", "hevc_videotoolbox"],
    ),
    ("libsvtav1", &["av1_nvenc", "av1_qsv", "av1_amf"]),
    ("libaom-av1", &["av1_nvenc", "av1_qsv", "av1_amf"]),
    ("libvpx-vp9", &["vp9_qsv"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwKind {
    Software,
    Nvenc,
    Qsv,
    Amf,
    Vaapi,
    Videotoolbox,
    MediaFoundation,
}

pub fn hw_kind(codec: &str) -> HwKind {
    if codec.ends_with("_nvenc") {
        HwKind::Nvenc
    } else if codec.ends_with("_qsv") {
        HwKind::Qsv
    } else if codec.ends_with("_amf") {
        HwKind::Amf
    } else if codec.ends_with("_vaapi") {
        HwKind::Vaapi
    } else if codec.ends_with("_videotoolbox") {
        HwKind::Videotoolbox
    } else if codec.ends_with("_mf") {
        HwKind::MediaFoundation
    } else {
        HwKind::Software
    }
}

/// Pick the concrete encoder for a configured `codec`.
///
/// Returns `(encoder, note)`; the note explains a substitution and is shown in
/// the command preview so the substitution is never silent.
pub fn resolve(
    codec: &str,
    prefer_hardware: bool,
    reg: Option<&EncoderRegistry>,
) -> (String, Option<String>) {
    if codec == "copy" {
        return (codec.to_string(), None);
    }
    let Some(reg) = reg else {
        return (codec.to_string(), None);
    };
    if reg.error.is_some() {
        return (codec.to_string(), None);
    }

    if prefer_hardware {
        if let Some((_, cands)) = HW_PREFERENCE.iter().find(|(sw, _)| *sw == codec) {
            if let Some(hw) = cands.iter().find(|c| reg.hw.get(**c) == Some(&true)) {
                return (
                    (*hw).to_string(),
                    Some(format!("{} » {} (hardware)", codec, hw)),
                );
            }
            if cands.iter().any(|c| reg.video.contains(*c)) {
                return (
                    codec.to_string(),
                    Some(format!(
                        "hardware encoder for {} is compiled in but not usable here — falling back to software",
                        codec
                    )),
                );
            }
        }
    }
    (codec.to_string(), None)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

fn no_window(cmd: &mut Command) {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

fn ffmpeg_version() -> String {
    let mut cmd = Command::new(ffmpeg_bin());
    cmd.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string(),
        Err(_) => String::new(),
    }
}

/// One-frame encode with `name`. `true` only if the process exits cleanly.
fn probe_encoder(name: &str) -> bool {
    let mut cmd = Command::new(ffmpeg_bin());
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=128x128:r=25:d=0.2",
        "-frames:v",
        "1",
        "-c:v",
        name,
        "-f",
        "null",
        "-",
    ]);
    no_window(&mut cmd);
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => return false,
        }
    }
}

/// Full detection: list encoders, then probe every hardware candidate that is
/// compiled in. Blocks; call it from a worker thread.
pub fn detect_blocking() -> EncoderRegistry {
    let mut reg = EncoderRegistry {
        version: ffmpeg_version(),
        detected_at: now_secs(),
        ..Default::default()
    };

    let mut cmd = Command::new(ffmpeg_bin());
    cmd.args(["-hide_banner", "-encoders"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let (v, a) = parse_encoders(&text);
            reg.video = v;
            reg.audio = a;
            if reg.video.is_empty() && reg.audio.is_empty() {
                reg.error = Some(format!(
                    "could not parse `{} -encoders` output",
                    ffmpeg_bin()
                ));
                return reg;
            }
        }
        Err(e) => {
            reg.error = Some(format!("cannot run {}: {}", ffmpeg_bin(), e));
            return reg;
        }
    }

    let candidates: Vec<&str> = HW_CANDIDATES
        .iter()
        .copied()
        .filter(|c| reg.video.contains(*c))
        .collect();
    if candidates.is_empty() {
        return reg;
    }

    // Probe in parallel: each probe is a full ffmpeg start-up (~0.7 s).
    let results: Mutex<Vec<(String, bool)>> = Mutex::new(Vec::with_capacity(candidates.len()));
    let next = AtomicUsize::new(0);
    let workers = candidates.len().min(4).max(1);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= candidates.len() {
                    break;
                }
                let name = candidates[i];
                let ok = probe_encoder(name);
                if let Ok(mut guard) = results.lock() {
                    guard.push((name.to_string(), ok));
                }
            });
        }
    });
    reg.hw = results.into_inner().unwrap_or_default().into_iter().collect();
    reg
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Cache + global slot
// ---------------------------------------------------------------------------

fn cache_path() -> PathBuf {
    crate::app::config_dir().join("encoders.json")
}

pub fn load_cache() -> Option<EncoderRegistry> {
    let text = std::fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_cache(reg: &EncoderRegistry) {
    if let Ok(json) = serde_json::to_string_pretty(reg) {
        let _ = std::fs::write(cache_path(), json);
    }
}

#[derive(Default)]
struct Slot {
    reg: Option<Arc<EncoderRegistry>>,
    running: bool,
    started: bool,
}

fn slot() -> &'static Mutex<Slot> {
    static SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Slot::default()))
}

/// Latest known registry (cached or freshly detected). Never blocks.
pub fn snapshot() -> Option<Arc<EncoderRegistry>> {
    slot().lock().ok().and_then(|s| s.reg.clone())
}

pub fn is_detecting() -> bool {
    slot().lock().map(|s| s.running).unwrap_or(false)
}

/// Start detection once per process. Returns immediately.
pub fn ensure_detection() {
    {
        let Ok(mut s) = slot().lock() else { return };
        if s.started {
            return;
        }
        s.started = true;
        s.running = true;
        if s.reg.is_none() {
            s.reg = load_cache().map(Arc::new);
        }
    }
    std::thread::spawn(|| {
        let reg = detect_blocking();
        save_cache(&reg);
        if let Ok(mut s) = slot().lock() {
            s.reg = Some(Arc::new(reg));
            s.running = false;
        }
    });
}

/// Force a fresh detection (the "re-detect" button). Ignored while running.
pub fn redetect() {
    {
        let Ok(mut s) = slot().lock() else { return };
        if s.running {
            return;
        }
        s.running = true;
        s.started = true;
    }
    std::thread::spawn(|| {
        let reg = detect_blocking();
        save_cache(&reg);
        if let Ok(mut s) = slot().lock() {
            s.reg = Some(Arc::new(reg));
            s.running = false;
        }
    });
}

/// Blocking variant used by `--selftest`.
pub fn detect_now() -> Arc<EncoderRegistry> {
    let reg = Arc::new(detect_blocking());
    save_cache(&reg);
    if let Ok(mut s) = slot().lock() {
        s.reg = Some(reg.clone());
        s.started = true;
        s.running = false;
    }
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_encoder_table() {
        let text = "\
Encoders:
 V..... = Video
 A..... = Audio
 ------
 V....D libx264              H.264 / AVC (codec h264)
 V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)
 A....D aac                  AAC (Advanced Audio Coding)
 A....D libopus              libopus Opus (codec opus)
 S..... srt                  SubRip subtitle
";
        let (v, a) = parse_encoders(text);
        assert!(v.contains("libx264"));
        assert!(v.contains("h264_nvenc"));
        assert!(a.contains("aac"));
        assert!(a.contains("libopus"));
        // subtitles and the legend lines are ignored
        assert!(!v.contains("srt"));
        assert!(!v.contains("="));
        assert_eq!(v.len(), 2);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn hw_kind_classification() {
        assert_eq!(hw_kind("h264_nvenc"), HwKind::Nvenc);
        assert_eq!(hw_kind("hevc_qsv"), HwKind::Qsv);
        assert_eq!(hw_kind("av1_amf"), HwKind::Amf);
        assert_eq!(hw_kind("h264_vaapi"), HwKind::Vaapi);
        assert_eq!(hw_kind("hevc_videotoolbox"), HwKind::Videotoolbox);
        assert_eq!(hw_kind("h264_mf"), HwKind::MediaFoundation);
        assert_eq!(hw_kind("libx264"), HwKind::Software);
    }

    #[test]
    fn resolve_prefers_verified_hardware() {
        let mut reg = EncoderRegistry::default();
        reg.video.insert("libx264".into());
        reg.video.insert("h264_nvenc".into());
        reg.video.insert("h264_qsv".into());
        reg.hw.insert("h264_nvenc".into(), true);
        reg.hw.insert("h264_qsv".into(), false);

        let (codec, note) = resolve("libx264", true, Some(&reg));
        assert_eq!(codec, "h264_nvenc");
        assert!(note.unwrap().contains("hardware"));

        // Opting out keeps the software encoder.
        let (codec, note) = resolve("libx264", false, Some(&reg));
        assert_eq!(codec, "libx264");
        assert!(note.is_none());
    }

    #[test]
    fn resolve_falls_back_when_hardware_unusable() {
        let mut reg = EncoderRegistry::default();
        reg.video.insert("libx264".into());
        reg.video.insert("h264_nvenc".into());
        reg.hw.insert("h264_nvenc".into(), false);
        let (codec, note) = resolve("libx264", true, Some(&reg));
        assert_eq!(codec, "libx264");
        assert!(note.unwrap().contains("falling back"));
    }

    #[test]
    fn resolve_without_registry_is_identity() {
        let (codec, note) = resolve("libx265", true, None);
        assert_eq!(codec, "libx265");
        assert!(note.is_none());
    }
}
