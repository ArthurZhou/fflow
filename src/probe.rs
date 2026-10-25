//! ffprobe wrapper: decides which Demux ports are available and provides the
//! total duration used for progress percentages.
//!
//! Everything goes through the `ffprobe` executable — no libav bindings.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// One elementary stream, as seen by ffprobe.
///
/// `ordinal` is the 0-based position *within its own type* — that is exactly
/// the `N` in ffmpeg's `0:v:N` / `0:a:N` / `0:s:N` stream specifier.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamInfo {
    pub ordinal: u32,
    /// Absolute stream index (`streams[].index`).
    pub index: u32,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub channels: u32,
    pub sample_rate: u32,
    pub bitrate: i64,
    pub language: String,
    pub title: String,
    pub default: bool,
}

impl StreamInfo {
    /// Compact one-line description for the track picker.
    pub fn describe(&self, video: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        if video {
            if self.width > 0 {
                parts.push(format!("{}x{}", self.width, self.height));
            }
            if self.fps > 0.0 {
                parts.push(format!("{:.2}fps", self.fps));
            }
        } else {
            if self.channels > 0 {
                parts.push(format!("{}ch", self.channels));
            }
            if self.sample_rate > 0 {
                parts.push(format!("{}Hz", self.sample_rate));
            }
        }
        parts.push(self.codec.clone());
        if !self.language.is_empty() && self.language != "und" {
            parts.push(self.language.clone());
        }
        if !self.title.is_empty() {
            parts.push(self.title.clone());
        }
        if self.default {
            parts.push("default".into());
        }
        parts.join(" · ")
    }
}

/// Result of probing one file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProbeInfo {
    pub path: String,
    pub duration: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub video_codec: String,
    pub audio_codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bitrate: i64,
    /// Every real video stream (cover art excluded), in ffmpeg ordinal order.
    #[serde(default)]
    pub video_streams: Vec<StreamInfo>,
    #[serde(default)]
    pub audio_streams: Vec<StreamInfo>,
    #[serde(default)]
    pub subtitle_streams: Vec<StreamInfo>,
    /// Container-level metadata (`format.tags`), keys lower-cased. Used by the
    /// Condition node so a workflow can branch on e.g. `comment` or `artist`.
    #[serde(default)]
    pub tags: std::collections::BTreeMap<String, String>,
}

impl ProbeInfo {
    pub fn resolution(&self) -> String {
        if self.has_video {
            format!("{}x{} @ {:.2}fps", self.width, self.height, self.fps)
        } else {
            "audio only".to_string()
        }
    }

    pub fn streams_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.has_video {
            parts.push(match self.video_streams.len() {
                n if n > 1 => format!("v:{}×{}", self.video_codec, n),
                _ => format!("v:{}", self.video_codec),
            });
        }
        if self.has_audio {
            parts.push(match self.audio_streams.len() {
                n if n > 1 => format!("a:{}×{}", self.audio_codec, n),
                _ => format!("a:{}", self.audio_codec),
            });
        }
        if !self.subtitle_streams.is_empty() {
            parts.push(format!("s×{}", self.subtitle_streams.len()));
        }
        parts.join(" + ")
    }
}

pub fn ffprobe_bin() -> String {
    std::env::var("FFLOW_FFPROBE").unwrap_or_else(|_| "ffprobe".to_string())
}

/// Message sent back to the UI thread when a background probe finishes.
#[derive(Debug)]
pub struct ProbeResult {
    pub path: std::path::PathBuf,
    pub result: Result<ProbeInfo>,
}

pub fn ffmpeg_bin() -> String {
    std::env::var("FFLOW_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

/// Parse the JSON printed by
/// `ffprobe -v quiet -print_format json -show_format -show_streams`.
pub fn parse_probe_json(text: &str, path: &str) -> Result<ProbeInfo> {
    let v: serde_json::Value = serde_json::from_str(text).context("ffprobe returned invalid JSON")?;
    let mut info = ProbeInfo {
        path: path.to_string(),
        ..Default::default()
    };

    if let Some(streams) = v.get("streams").and_then(|s| s.as_array()) {
        let (mut v_ord, mut a_ord, mut s_ord) = (0u32, 0u32, 0u32);
        for s in streams {
            let index = s.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let codec = s
                .get("codec_name")
                .and_then(|c| c.as_str())
                .unwrap_or("?")
                .to_string();
            let tags = s.get("tags");
            let tag = |k: &str| -> String {
                tags.and_then(|t| t.get(k))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let disposition = s.get("disposition");
            let disp = |k: &str| -> bool {
                disposition.and_then(|d| d.get(k)).and_then(|d| d.as_u64()) == Some(1)
            };
            let attached_pic = disp("attached_pic");
            let is_default = disp("default");

            match s.get("codec_type").and_then(|c| c.as_str()) {
                Some("video") => {
                    let stream = StreamInfo {
                        ordinal: v_ord,
                        index,
                        codec: codec.clone(),
                        width: s.get("width").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        height: s.get("height").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        fps: parse_fraction(s.get("avg_frame_rate"))
                            .or_else(|| parse_fraction(s.get("r_frame_rate")))
                            .unwrap_or(0.0),
                        bitrate: s
                            .get("bit_rate")
                            .and_then(|d| d.as_str())
                            .and_then(|d| d.parse::<i64>().ok())
                            .unwrap_or(0),
                        language: tag("language"),
                        title: tag("title"),
                        default: is_default,
                        ..Default::default()
                    };
                    // Ordinals follow ffmpeg's numbering, which counts cover
                    // art as a video stream even though we hide it.
                    v_ord += 1;
                    if attached_pic {
                        continue;
                    }
                    if !info.has_video {
                        info.has_video = true;
                        info.video_codec = stream.codec.clone();
                        info.width = stream.width;
                        info.height = stream.height;
                        info.fps = stream.fps;
                    }
                    info.video_streams.push(stream);
                }
                Some("audio") => {
                    let stream = StreamInfo {
                        ordinal: a_ord,
                        index,
                        codec: codec.clone(),
                        channels: s.get("channels").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        sample_rate: s
                            .get("sample_rate")
                            .and_then(|d| d.as_str())
                            .and_then(|d| d.parse::<u32>().ok())
                            .unwrap_or(0),
                        bitrate: s
                            .get("bit_rate")
                            .and_then(|d| d.as_str())
                            .and_then(|d| d.parse::<i64>().ok())
                            .unwrap_or(0),
                        language: tag("language"),
                        title: tag("title"),
                        default: is_default,
                        ..Default::default()
                    };
                    a_ord += 1;
                    if !info.has_audio {
                        info.has_audio = true;
                        info.audio_codec = stream.codec.clone();
                    }
                    info.audio_streams.push(stream);
                }
                Some("subtitle") => {
                    let stream = StreamInfo {
                        ordinal: s_ord,
                        index,
                        codec,
                        language: tag("language"),
                        title: tag("title"),
                        default: is_default,
                        ..Default::default()
                    };
                    s_ord += 1;
                    info.subtitle_streams.push(stream);
                }
                _ => {}
            }
        }
    }

    if let Some(fmt) = v.get("format") {
        info.duration = fmt
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<f64>().ok())
            .unwrap_or(0.0);
        info.bitrate = fmt
            .get("bit_rate")
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<i64>().ok())
            .unwrap_or(0);
        if let Some(tags) = fmt.get("tags").and_then(|t| t.as_object()) {
            for (k, v) in tags {
                if let Some(v) = v.as_str() {
                    info.tags
                        .insert(k.to_ascii_lowercase(), v.to_string());
                }
            }
        }
    }

    if info.duration <= 0.0 {
        if let Some(streams) = v.get("streams").and_then(|s| s.as_array()) {
            for s in streams {
                if let Some(d) = s
                    .get("duration")
                    .and_then(|d| d.as_str())
                    .and_then(|d| d.parse::<f64>().ok())
                {
                    info.duration = info.duration.max(d);
                }
            }
        }
    }

    Ok(info)
}

fn parse_fraction(v: Option<&serde_json::Value>) -> Option<f64> {
    let s = v?.as_str()?;
    let (num, den) = s.split_once('/')?;
    let n: f64 = num.parse().ok()?;
    let d: f64 = den.parse().ok()?;
    if d == 0.0 {
        return None;
    }
    Some(n / d)
}

/// Run ffprobe on `path` (async, never blocks the UI thread).
pub async fn probe(path: &Path) -> Result<ProbeInfo> {
    let mut cmd = Command::new(ffprobe_bin());
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(path)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let out = cmd
        .output()
        .await
        .with_context(|| format!("failed to start {}", ffprobe_bin()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!(
            "ffprobe failed for {}: {}",
            path.display(),
            err.lines().last().unwrap_or("unknown error")
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_probe_json(&text, &path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_and_audio() {
        let json = r#"{
            "streams": [
                {"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
                 "avg_frame_rate":"30000/1001","disposition":{"attached_pic":0}},
                {"codec_type":"audio","codec_name":"aac","disposition":{"attached_pic":0}}
            ],
            "format": {"duration":"12.345","bit_rate":"1234567"}
        }"#;
        let info = parse_probe_json(json, "x.mp4").unwrap();
        assert!(info.has_video && info.has_audio);
        assert_eq!(info.width, 1920);
        assert!((info.fps - 29.97).abs() < 0.05);
        assert!((info.duration - 12.345).abs() < 1e-6);
    }

    #[test]
    fn audio_only_file() {
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"mp3"}],
                       "format":{"duration":"5.0"}}"#;
        let info = parse_probe_json(json, "x.mp3").unwrap();
        assert!(!info.has_video);
        assert!(info.has_audio);
    }

    #[test]
    fn collects_every_track_with_ordinals() {
        let json = r#"{
            "streams": [
                {"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,
                 "avg_frame_rate":"25/1","disposition":{"default":1,"attached_pic":0}},
                {"index":1,"codec_type":"audio","codec_name":"aac","channels":2,
                 "sample_rate":"48000","tags":{"language":"jpn","title":"Main"},
                 "disposition":{"default":1,"attached_pic":0}},
                {"index":2,"codec_type":"audio","codec_name":"aac","channels":6,
                 "sample_rate":"48000","tags":{"language":"eng"},
                 "disposition":{"default":0,"attached_pic":0}},
                {"index":3,"codec_type":"subtitle","codec_name":"subrip",
                 "tags":{"language":"eng"},"disposition":{"default":0,"attached_pic":0}},
                {"index":4,"codec_type":"subtitle","codec_name":"ass",
                 "disposition":{"default":0,"attached_pic":0}},
                {"index":5,"codec_type":"video","codec_name":"mjpeg","width":600,"height":600,
                 "avg_frame_rate":"0/0","disposition":{"default":0,"attached_pic":1}}
            ],
            "format": {"duration":"60.0","bit_rate":"1000000"}
        }"#;
        let info = parse_probe_json(json, "x.mkv").unwrap();

        assert_eq!(info.video_streams.len(), 1, "cover art must be hidden");
        assert_eq!(info.video_streams[0].ordinal, 0);
        assert_eq!(info.video_streams[0].width, 1920);

        assert_eq!(info.audio_streams.len(), 2);
        assert_eq!(info.audio_streams[1].ordinal, 1);
        assert_eq!(info.audio_streams[1].channels, 6);
        assert_eq!(info.audio_streams[1].language, "eng");
        assert_eq!(info.audio_streams[0].title, "Main");

        assert_eq!(info.subtitle_streams.len(), 2);
        assert_eq!(info.subtitle_streams[1].codec, "ass");

        // summary mentions the multiplicity
        assert!(info.streams_summary().contains("a:aac×2"));
        assert!(info.streams_summary().contains("s×2"));
    }

    #[test]
    fn cover_art_ordinal_is_still_counted() {
        // A file whose only real video stream comes *after* the cover art must
        // keep ffmpeg's numbering, i.e. the real stream is `0:v:1`.
        let json = r#"{"streams":[
            {"index":0,"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}},
            {"index":1,"codec_type":"video","codec_name":"h264","width":640,"height":360,
             "disposition":{"attached_pic":0}}
        ],"format":{"duration":"1.0"}}"#;
        let info = parse_probe_json(json, "x.mp4").unwrap();
        assert_eq!(info.video_streams.len(), 1);
        assert_eq!(info.video_streams[0].ordinal, 1);
    }

    #[test]
    fn track_mode_resolution() {
        use crate::graph::nodes::TrackMode;
        assert_eq!(TrackMode::First.resolve(&[], 3), vec![0]);
        assert_eq!(TrackMode::All.resolve(&[], 3), vec![0, 1, 2]);
        assert_eq!(TrackMode::Off.resolve(&[0, 1], 3), Vec::<u32>::new());
        assert_eq!(TrackMode::Select.resolve(&[2, 0, 0], 3), vec![0, 2]);
        // out-of-range picks are dropped
        assert_eq!(TrackMode::Select.resolve(&[0, 9], 2), vec![0]);
        // an empty pick must not produce an empty stream
        assert_eq!(TrackMode::Select.resolve(&[], 2), vec![0]);
        // a type with no streams yields nothing even for First
        assert_eq!(TrackMode::First.resolve(&[], 0), Vec::<u32>::new());
    }
}
