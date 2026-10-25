//! Node kinds, their typed configuration, and the property-panel forms.
//!
//! Every node type keeps a *typed* config in memory and is serialized to an
//! opaque `serde_json::Value` when the workflow is saved, so that the JSON
//! document stays stable even if a config struct gains fields.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::StreamType;

mod filters;
pub use filters::*;

pub const DEFAULT_EXTENSIONS: &str =
    "mp4,mkv,mov,m4v,avi,webm,ts,flv,wmv,mp3,m4a,aac,wav,flac,ogg,opus,wma";

// ---------------------------------------------------------------------------
// Node kind
// ---------------------------------------------------------------------------

/// What a card does to the media flowing through it.
///
/// The colour family is independent of the library category: `Scale` and
/// `Volume` both live under "Filters" but keep the video/audio accent so a
/// chain is readable at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    File,
    Video,
    Audio,
    Logic,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    // input
    Source,
    Demux,
    // video
    Scale,
    Pad,
    Crop,
    Rotate,
    Fps,
    Color,
    Lut,
    Sharpen,
    Denoise,
    Overlay,
    // audio
    Volume,
    Loudness,
    Band,
    Tempo,
    Channels,
    Resample,
    // both
    RawFilter,
    // logic
    Condition,
    // output
    VideoEncoder,
    AudioEncoder,
    Package,
}

/// Everything the library, the ports and the serializer need to know about a
/// kind, in one place.
///
/// Keeping this as a table instead of a dozen `match` arms means a new card is
/// one entry plus its config, and the library order is the table order.
pub struct KindSpec {
    pub kind: NodeKind,
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub blurb: &'static str,
    /// Extra search words; the library matches these as well as the title.
    pub keywords: &'static str,
    pub family: Family,
    pub inputs: &'static [(&'static str, StreamType)],
    pub outputs: &'static [(&'static str, StreamType)],
    /// Merge point: the input accepts several connections and the compiler
    /// picks the live one per file. Used by the encoders and Package, so the
    /// two sides of a Condition can re-join without an extra node.
    pub fan_in: bool,
}

use StreamType::{Any, Audio, File, Video};

pub const SPECS: &[KindSpec] = &[
    KindSpec {
        kind: NodeKind::Source,
        id: "source",
        title: "Source",
        category: "Input",
        blurb: "Root folder(s) / dropped files",
        keywords: "folder directory files input scan recursive",
        family: Family::File,
        inputs: &[],
        outputs: &[("file", File)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Demux,
        id: "demux",
        title: "Demux",
        category: "Input",
        blurb: "Split a file into video + audio",
        keywords: "tracks streams split separate video audio subtitle",
        family: Family::File,
        inputs: &[("in", File)],
        outputs: &[("video", Video), ("audio", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Scale,
        id: "scale",
        title: "Scale",
        category: "Filters",
        blurb: "Resolution: exact / fit / fill",
        keywords: "resolution resize downscale upscale 1080p 720p 480p 4k size",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Pad,
        id: "pad",
        title: "Pad",
        category: "Filters",
        blurb: "Letterbox to an exact canvas",
        keywords: "letterbox border canvas aspect bars black background",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Crop,
        id: "crop",
        title: "Crop",
        category: "Filters",
        blurb: "Cut a rectangle out of the frame",
        keywords: "trim frame region rectangle widescreen",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Rotate,
        id: "rotate",
        title: "Rotate / Flip",
        category: "Filters",
        blurb: "90° steps and mirroring",
        keywords: "rotation orientation portrait transpose mirror hflip vflip",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Fps,
        id: "fps",
        title: "Frame Rate",
        category: "Filters",
        blurb: "Resample the frame rate",
        keywords: "fps frame rate cadence 24 25 30 60 ntsc",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Color,
        id: "color",
        title: "Colour",
        category: "Filters",
        blurb: "Brightness / contrast / saturation",
        keywords: "eq color grade brightness contrast saturation gamma exposure",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Lut,
        id: "lut",
        title: "3D LUT",
        category: "Filters",
        blurb: ".cube / .3dl look-up table",
        keywords: "lut3d cube 3dl grade film emulation look",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Sharpen,
        id: "sharpen",
        title: "Sharpen",
        category: "Filters",
        blurb: "Unsharp mask",
        keywords: "unsharp detail crisp clarity",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Denoise,
        id: "denoise",
        title: "Denoise",
        category: "Filters",
        blurb: "Temporal + spatial noise reduction",
        keywords: "hqdn3d grain noise smooth",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Overlay,
        id: "overlay",
        title: "Overlay",
        category: "Filters",
        blurb: "Burn an image onto the video",
        keywords: "watermark logo image png composite stamp",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Volume,
        id: "volume",
        title: "Volume",
        category: "Filters",
        blurb: "Gain in dB",
        keywords: "gain loud quiet audio level db amplify",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Loudness,
        id: "loudness",
        title: "Loudness",
        category: "Filters",
        blurb: "Normalise to a LUFS target",
        keywords: "loudnorm lufs normalize podcast broadcast streaming",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Band,
        id: "band",
        title: "Band Filter",
        category: "Filters",
        blurb: "High-pass / low-pass",
        keywords: "highpass lowpass rumble hiss cut eq frequency",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Tempo,
        id: "tempo",
        title: "Tempo",
        category: "Filters",
        blurb: "Speed up / slow down audio",
        keywords: "atempo speed slow fast pitch time stretch",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Channels,
        id: "channels",
        title: "Channels",
        category: "Filters",
        blurb: "Mono / stereo / 5.1 layout",
        keywords: "aformat channel layout mono stereo surround 5.1 downmix",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Resample,
        id: "resample",
        title: "Resample",
        category: "Filters",
        blurb: "Change the sample rate",
        keywords: "aresample sample rate 44100 48000 96k hz",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::RawFilter,
        id: "raw_filter",
        title: "Raw Filter",
        category: "Filters",
        blurb: "Verbatim filtergraph fragment",
        keywords: "custom advanced escape hatch setpts aecho expression",
        family: Family::File,
        inputs: &[("in", Any)],
        outputs: &[("out", Any)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::Condition,
        id: "condition",
        title: "Condition",
        category: "Logic",
        blurb: "Route each file to pass or fail",
        keywords: "branch if else filter name metadata tag size duration switch",
        family: Family::Logic,
        inputs: &[("in", Any)],
        outputs: &[("pass", Any), ("fail", Any)],
        fan_in: false,
    },
    KindSpec {
        kind: NodeKind::VideoEncoder,
        id: "video_encoder",
        title: "Video Encoder",
        category: "Encoders",
        blurb: "libx264 / libx265 / vp9 ...",
        keywords: "codec h264 h265 hevc av1 vp9 hardware nvenc quality crf",
        family: Family::Video,
        inputs: &[("in", Video)],
        outputs: &[("out", Video)],
        fan_in: true,
    },
    KindSpec {
        kind: NodeKind::AudioEncoder,
        id: "audio_encoder",
        title: "Audio Encoder",
        category: "Encoders",
        blurb: "aac / libopus / mp3 ...",
        keywords: "codec aac opus mp3 bitrate quality copy",
        family: Family::Audio,
        inputs: &[("in", Audio)],
        outputs: &[("out", Audio)],
        fan_in: true,
    },
    KindSpec {
        kind: NodeKind::Package,
        id: "package",
        title: "Package",
        category: "Output",
        blurb: "Container, flags, naming template",
        keywords: "container mux mp4 mkv output naming template metadata",
        family: Family::Output,
        inputs: &[("video", Video), ("audio", Audio)],
        outputs: &[("file", File)],
        fan_in: true,
    },
];

impl NodeKind {
    pub const ALL: [NodeKind; 23] = [
        NodeKind::Source,
        NodeKind::Demux,
        NodeKind::Scale,
        NodeKind::Pad,
        NodeKind::Crop,
        NodeKind::Rotate,
        NodeKind::Fps,
        NodeKind::Color,
        NodeKind::Lut,
        NodeKind::Sharpen,
        NodeKind::Denoise,
        NodeKind::Overlay,
        NodeKind::Volume,
        NodeKind::Loudness,
        NodeKind::Band,
        NodeKind::Tempo,
        NodeKind::Channels,
        NodeKind::Resample,
        NodeKind::RawFilter,
        NodeKind::Condition,
        NodeKind::VideoEncoder,
        NodeKind::AudioEncoder,
        NodeKind::Package,
    ];

    pub fn spec(&self) -> &'static KindSpec {
        SPECS
            .iter()
            .find(|s| s.kind == *self)
            .expect("every kind has a spec")
    }

    /// Stable string used in the JSON document.
    pub fn id(&self) -> &'static str {
        self.spec().id
    }

    pub fn from_id(s: &str) -> Option<Self> {
        SPECS.iter().find(|k| k.id == s).map(|k| k.kind)
    }

    pub fn title(&self) -> &'static str {
        self.spec().title
    }

    pub fn category(&self) -> &'static str {
        self.spec().category
    }

    pub fn blurb(&self) -> &'static str {
        self.spec().blurb
    }

    pub fn keywords(&self) -> &'static str {
        self.spec().keywords
    }

    /// Case-insensitive search over everything a card advertises.
    ///
    /// The keyword list matters more than the title: people look for "resize",
    /// "loudness" or "watermark", which are not the words on the card.
    pub fn matches(&self, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        let hay = format!(
            "{} {} {} {}",
            self.title(),
            self.blurb(),
            self.category(),
            self.keywords()
        )
        .to_ascii_lowercase();
        needle.split_whitespace().all(|word| hay.contains(word))
    }

    pub fn family(&self) -> Family {
        self.spec().family
    }

    /// (inputs, outputs) with their port types. Port names are part of the
    /// saved workflow format, do not rename them.
    pub fn ports(&self) -> (Vec<(&'static str, StreamType)>, Vec<(&'static str, StreamType)>) {
        (self.spec().inputs.to_vec(), self.spec().outputs.to_vec())
    }

    pub fn fan_in(&self) -> bool {
        self.spec().fan_in
    }

    pub fn default_config(&self) -> NodeConfig {
        NodeConfig::default_for(*self)
    }
}

// ---------------------------------------------------------------------------
// Configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
    /// Directories or individual files. Directories are walked (optionally
    /// recursively) and filtered by extension.
    pub paths: Vec<String>,
    pub recursive: bool,
    pub extensions: String,
    /// Skip files that fflow itself produced (`*_fflow.*` / `fflow_out/`) so a
    /// second run does not re-process its own results.
    #[serde(default = "default_true")]
    pub skip_own_outputs: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            recursive: true,
            extensions: DEFAULT_EXTENSIONS.to_string(),
            skip_own_outputs: true,
        }
    }
}

/// Which tracks of one media type the Demux node forwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TrackMode {
    /// Only the first stream of that type (ffmpeg's `0:v:0`).
    #[default]
    First,
    /// Every stream of that type.
    All,
    /// Only the ordinals listed in the matching `*_tracks` vector.
    Select,
    /// Drop this media type entirely.
    Off,
}

impl TrackMode {
    pub const ALL: [TrackMode; 4] = [
        TrackMode::First,
        TrackMode::All,
        TrackMode::Select,
        TrackMode::Off,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            TrackMode::First => "first",
            TrackMode::All => "all",
            TrackMode::Select => "pick",
            TrackMode::Off => "off",
        }
    }

    /// Resolve to the concrete ordinals to forward.
    ///
    /// `selected` is only consulted for [`TrackMode::Select`]; ordinals beyond
    /// `total` are dropped so a workflow built on a 2-track file still works on
    /// a 1-track file.
    pub fn resolve(&self, selected: &[u32], total: usize) -> Vec<u32> {
        match self {
            TrackMode::Off => Vec::new(),
            TrackMode::All => (0..total as u32).collect(),
            TrackMode::First => {
                if total == 0 {
                    Vec::new()
                } else {
                    vec![0]
                }
            }
            TrackMode::Select => {
                let mut v: Vec<u32> = selected.iter().copied().filter(|t| (*t as usize) < total).collect();
                v.sort_unstable();
                v.dedup();
                if v.is_empty() && total > 0 {
                    // An empty pick would silently produce an empty output.
                    vec![0]
                } else {
                    v
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DemuxConfig {
    pub video_mode: TrackMode,
    /// 0-based video ordinals used when `video_mode == Select`.
    pub video_tracks: Vec<u32>,
    pub audio_mode: TrackMode,
    pub audio_tracks: Vec<u32>,
    pub subtitle_mode: TrackMode,
    pub subtitle_tracks: Vec<u32>,
}

impl Default for DemuxConfig {
    fn default() -> Self {
        Self {
            video_mode: TrackMode::First,
            video_tracks: Vec::new(),
            audio_mode: TrackMode::First,
            audio_tracks: Vec::new(),
            subtitle_mode: TrackMode::Off,
            subtitle_tracks: Vec::new(),
        }
    }
}

impl DemuxConfig {
    /// True when every media type uses the default selection (first track of
    /// video and audio, subtitles off).
    pub fn is_default_modes(&self) -> bool {
        self.video_mode == TrackMode::First
            && self.audio_mode == TrackMode::First
            && self.subtitle_mode == TrackMode::Off
    }

    pub fn summary(&self) -> String {
        let v = match self.video_mode {
            TrackMode::Off => "no video".to_string(),
            TrackMode::First => "v:1".to_string(),
            TrackMode::All => "v:all".to_string(),
            TrackMode::Select => format!("v:{}", self.video_tracks.len()),
        };
        let a = match self.audio_mode {
            TrackMode::Off => "no audio".to_string(),
            TrackMode::First => "a:1".to_string(),
            TrackMode::All => "a:all".to_string(),
            TrackMode::Select => format!("a:{}", self.audio_tracks.len()),
        };
        let s = match self.subtitle_mode {
            TrackMode::Off => None,
            TrackMode::First => Some("s:1".to_string()),
            TrackMode::All => Some("s:all".to_string()),
            TrackMode::Select => Some(format!("s:{}", self.subtitle_tracks.len())),
        };
        match s {
            Some(s) => format!("{} · {} · {}", v, a, s),
            None => format!("{} · {}", v, a),
        }
    }
}

// ---------------------------------------------------------------------------
// Condition (branch) node
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CondField {
    #[default]
    FileName,
    Extension,
    Path,
    Size,
    Duration,
    Width,
    Height,
    Fps,
    VideoCodec,
    AudioCodec,
    Bitrate,
    HasVideo,
    HasAudio,
    Tag,
}

impl CondField {
    pub const ALL: [CondField; 14] = [
        CondField::FileName,
        CondField::Extension,
        CondField::Path,
        CondField::Size,
        CondField::Duration,
        CondField::Width,
        CondField::Height,
        CondField::Fps,
        CondField::VideoCodec,
        CondField::AudioCodec,
        CondField::Bitrate,
        CondField::HasVideo,
        CondField::HasAudio,
        CondField::Tag,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            CondField::FileName => "file name",
            CondField::Extension => "extension",
            CondField::Path => "full path",
            CondField::Size => "size (MB)",
            CondField::Duration => "duration (s)",
            CondField::Width => "width",
            CondField::Height => "height",
            CondField::Fps => "fps",
            CondField::VideoCodec => "video codec",
            CondField::AudioCodec => "audio codec",
            CondField::Bitrate => "bitrate (kbps)",
            CondField::HasVideo => "has video",
            CondField::HasAudio => "has audio",
            CondField::Tag => "metadata",
        }
    }

    /// Numeric fields only accept comparisons, text fields only string ops.
    pub fn is_number(&self) -> bool {
        matches!(
            self,
            CondField::Size
                | CondField::Duration
                | CondField::Width
                | CondField::Height
                | CondField::Fps
                | CondField::Bitrate
                | CondField::HasVideo
                | CondField::HasAudio
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CondOp {
    #[default]
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Equals,
    Glob,
    Greater,
    Less,
}

impl CondOp {
    pub const TEXT: [CondOp; 6] = [
        CondOp::Contains,
        CondOp::NotContains,
        CondOp::StartsWith,
        CondOp::EndsWith,
        CondOp::Equals,
        CondOp::Glob,
    ];
    pub const NUMBER: [CondOp; 3] = [CondOp::Equals, CondOp::Greater, CondOp::Less];

    pub fn is_numeric(&self) -> bool {
        matches!(self, CondOp::Greater | CondOp::Less)
    }

    pub fn label(&self) -> &'static str {
        match self {
            CondOp::Contains => "contains",
            CondOp::NotContains => "does not contain",
            CondOp::StartsWith => "starts with",
            CondOp::EndsWith => "ends with",
            CondOp::Equals => "is",
            CondOp::Glob => "matches",
            CondOp::Greater => ">",
            CondOp::Less => "<",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CondRule {
    pub field: CondField,
    pub op: CondOp,
    pub value: String,
    /// Metadata key, only used when `field == CondField::Tag`.
    pub key: String,
}

impl CondRule {
    fn describe(&self) -> String {
        if self.field == CondField::Tag {
            return format!(
                "{} {} {}",
                if self.key.trim().is_empty() { "tag" } else { self.key.trim() },
                self.op.label(),
                self.value
            );
        }
        if self.field.is_number() {
            return format!("{} {} {}", self.field.label(), self.op.label(), self.value);
        }
        format!("{} {} \"{}\"", self.field.label(), self.op.label(), self.value)
    }
}

/// Route a file to the `pass` or `fail` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConditionConfig {
    pub rules: Vec<CondRule>,
    /// `true` = every rule must match (AND), `false` = any (OR).
    pub match_all: bool,
    /// Free-form name shown on the node, e.g. "intro clip".
    pub label: String,
}

impl Default for ConditionConfig {
    fn default() -> Self {
        Self {
            rules: vec![CondRule {
                field: CondField::Extension,
                op: CondOp::Equals,
                value: "mp4".into(),
                key: String::new(),
            }],
            match_all: true,
            label: String::new(),
        }
    }
}

/// Everything a condition can look at for one file.
pub struct FileFacts<'a> {
    pub path: &'a std::path::Path,
    pub size: u64,
    pub info: &'a crate::probe::ProbeInfo,
}

impl ConditionConfig {
    pub fn matches(&self, facts: &FileFacts<'_>) -> bool {
        if self.rules.is_empty() {
            return true;
        }
        if self.match_all {
            self.rules.iter().all(|r| rule_matches(r, facts))
        } else {
            self.rules.iter().any(|r| rule_matches(r, facts))
        }
    }

    pub fn summary(&self) -> String {
        if self.rules.is_empty() {
            return "always pass".into();
        }
        let joiner = if self.match_all { " and " } else { " or " };
        let mut s = self
            .rules
            .iter()
            .map(|r| r.describe())
            .collect::<Vec<_>>()
            .join(joiner);
        if s.chars().count() > 44 {
            s = format!("{}…", s.chars().take(43).collect::<String>());
        }
        s
    }
}

fn text_of(field: CondField, facts: &FileFacts<'_>) -> String {
    let path = facts.path;
    match field {
        CondField::FileName => path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        CondField::Extension => path
            .extension()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        CondField::Path => path.to_string_lossy().to_string(),
        CondField::VideoCodec => facts.info.video_codec.clone(),
        CondField::AudioCodec => facts.info.audio_codec.clone(),
        _ => String::new(),
    }
}

fn number_of(field: CondField, facts: &FileFacts<'_>) -> f64 {
    match field {
        CondField::Size => facts.size as f64 / (1024.0 * 1024.0),
        CondField::Duration => facts.info.duration,
        CondField::Width => facts.info.width as f64,
        CondField::Height => facts.info.height as f64,
        CondField::Fps => facts.info.fps,
        CondField::Bitrate => facts.info.bitrate as f64 / 1000.0,
        CondField::HasVideo => f64::from(u8::from(facts.info.has_video)),
        CondField::HasAudio => f64::from(u8::from(facts.info.has_audio)),
        _ => 0.0,
    }
}

fn rule_matches(rule: &CondRule, facts: &FileFacts<'_>) -> bool {
    if rule.field == CondField::Tag {
        let key = rule.key.trim().to_ascii_lowercase();
        if key.is_empty() {
            return false;
        }
        let value = facts
            .info
            .tags
            .get(&key)
            .cloned()
            .unwrap_or_default();
        return text_matches(rule.op, &value, rule.value.trim());
    }
    if rule.field.is_number() {
        let lhs = number_of(rule.field, facts);
        // `has video is true` reads better than `has video > 0`.
        let raw = rule.value.trim().to_ascii_lowercase();
        let rhs = match raw.as_str() {
            "true" | "yes" | "on" => 1.0,
            "false" | "no" | "off" => 0.0,
            _ => match raw.parse::<f64>() {
                Ok(v) => v,
                Err(_) => return false,
            },
        };
        return match rule.op {
            CondOp::Greater => lhs > rhs,
            CondOp::Less => lhs < rhs,
            _ => (lhs - rhs).abs() < 1e-6,
        };
    }
    text_matches(rule.op, &text_of(rule.field, facts), rule.value.trim())
}

fn text_matches(op: CondOp, haystack: &str, needle: &str) -> bool {
    let h = haystack.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    match op {
        CondOp::Contains => h.contains(&n),
        CondOp::NotContains => !h.contains(&n),
        CondOp::StartsWith => h.starts_with(&n),
        CondOp::EndsWith => h.ends_with(&n),
        CondOp::Equals => h == n,
        CondOp::Glob => glob_match(&n, &h),
        CondOp::Greater | CondOp::Less => false,
    }
}

/// Case-insensitive `*` / `?` glob match, no dependencies.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut resume) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            resume = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            resume += 1;
            ti = resume;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoEncoderConfig {
    pub codec: String,
    pub crf: u32,
    pub preset: String,
    pub bitrate: String,
    pub pix_fmt: String,
    /// Use `-b:v` instead of `-crf`.
    pub use_bitrate: bool,
    /// Substitute a verified hardware encoder for the configured software one
    /// (e.g. `libx264` → `h264_nvenc`) when the machine supports it.
    #[serde(default = "default_true")]
    pub prefer_hardware: bool,
}

impl Default for VideoEncoderConfig {
    fn default() -> Self {
        Self {
            codec: "libx264".into(),
            crf: 23,
            preset: "medium".into(),
            bitrate: "4M".into(),
            pix_fmt: "yuv420p".into(),
            use_bitrate: false,
            prefer_hardware: true,
        }
    }
}

pub const VIDEO_CODECS: &[&str] = &[
    "libx264",
    "libx265",
    "libvpx-vp9",
    "libsvtav1",
    "h264_nvenc",
    "hevc_nvenc",
    "av1_nvenc",
    "h264_qsv",
    "hevc_qsv",
    "h264_amf",
    "hevc_amf",
    "h264_videotoolbox",
    "hevc_videotoolbox",
    "h264_vaapi",
    "hevc_vaapi",
    "mpeg4",
    "copy",
];

pub const PRESETS: &[&str] = &[
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

impl VideoEncoderConfig {
    /// The encoder that will actually be passed to `-c:v`, plus an explanatory
    /// note when hardware substitution happened.
    pub fn effective_codec(
        &self,
        reg: Option<&crate::encoders::EncoderRegistry>,
    ) -> (String, Option<String>) {
        crate::encoders::resolve(&self.codec, self.prefer_hardware, reg)
    }

    /// Encoder arguments for a *resolved* codec name.
    ///
    /// Quality/preset flags differ per encoder family: NVENC has `-cq`,
    /// QSV `-global_quality`, AMF `-qp_i`, VAAPI `-qp`, and the two Apple /
    /// Windows wrappers a 1..100 quality scale. Emitting `-crf` for all of
    /// them would silently produce garbage.
    pub fn args_for(&self, codec: &str) -> Vec<String> {
        use crate::encoders::{hw_kind, HwKind};

        let mut a: Vec<String> = vec!["-c:v".into(), codec.to_string()];
        if codec == "copy" {
            return a;
        }
        let kind = hw_kind(codec);
        let crf = self.crf.to_string();

        if self.use_bitrate {
            if !self.bitrate.trim().is_empty() {
                a.push("-b:v".into());
                a.push(self.bitrate.trim().to_string());
            }
            match kind {
                HwKind::Nvenc => a.extend(["-rc".to_string(), "vbr".to_string()]),
                HwKind::Amf => a.extend(["-rc".to_string(), "vbr_peak".to_string()]),
                HwKind::Vaapi => a.extend(["-rc_mode".to_string(), "CBR".to_string()]),
                _ => {}
            }
        } else {
            match kind {
                HwKind::Nvenc => a.extend(
                    ["-rc", "vbr", "-cq", crf.as_str(), "-b:v", "0"].map(String::from),
                ),
                HwKind::Qsv => a.extend(["-global_quality".to_string(), crf.clone()]),
                HwKind::Amf => a.extend(
                    ["-rc", "cqp", "-qp_i", crf.as_str(), "-qp_p", crf.as_str()].map(String::from),
                ),
                HwKind::Vaapi => a.extend(["-rc_mode".to_string(), "CQP".to_string(), "-qp".to_string(), crf.clone()]),
                HwKind::Videotoolbox | HwKind::MediaFoundation => {
                    // Both use a 1..100 scale where higher means better.
                    let q = (100i64 - self.crf as i64 * 2).clamp(1, 100).to_string();
                    if kind == HwKind::MediaFoundation {
                        a.extend(["-rate_control".to_string(), "3".to_string()]);
                    }
                    a.extend(["-q:v".to_string(), q]);
                }
                HwKind::Software => {
                    a.extend(["-crf".to_string(), crf.clone()]);
                    // VP9 needs an explicit bitrate target of 0 for constant quality.
                    if codec == "libvpx-vp9" {
                        a.extend(["-b:v".to_string(), "0".to_string()]);
                        a.extend(["-row-mt".to_string(), "1".to_string()]);
                    }
                }
            }
        }

        a.extend(self.preset_args(codec));

        // VAAPI needs an nv12 upload; forcing yuv420p would break the graph.
        if !self.pix_fmt.trim().is_empty() && kind != HwKind::Vaapi {
            a.push("-pix_fmt".into());
            a.push(self.pix_fmt.trim().to_string());
        }
        a
    }

    /// Encoder-family specific preset flags.
    fn preset_args(&self, codec: &str) -> Vec<String> {
        use crate::encoders::{hw_kind, HwKind};

        let p = self.preset.trim();
        if p.is_empty() {
            return Vec::new();
        }
        match hw_kind(codec) {
            HwKind::Software => {
                if codec.starts_with("libx") {
                    vec!["-preset".into(), p.to_string()]
                } else {
                    Vec::new()
                }
            }
            // Legacy NVENC preset names work on every generation (p1..p7 need
            // Turing or newer).
            HwKind::Nvenc => {
                let mapped = match p {
                    "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "fast",
                    "medium" => "medium",
                    "slow" | "slower" | "veryslow" => "slow",
                    other => other,
                };
                vec!["-preset".into(), mapped.to_string()]
            }
            HwKind::Qsv => {
                let mapped = match p {
                    "ultrafast" | "superfast" => "veryfast",
                    other => other,
                };
                vec!["-preset".into(), mapped.to_string()]
            }
            HwKind::Amf => {
                let mapped = match p {
                    "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "speed",
                    "medium" => "balanced",
                    _ => "quality",
                };
                vec!["-quality".into(), mapped.to_string()]
            }
            _ => Vec::new(),
        }
    }

    pub fn summary(&self) -> String {
        if self.codec == "copy" {
            return "copy".into();
        }
        if self.use_bitrate {
            format!("{} {} (b:v)", self.codec, self.bitrate)
        } else {
            format!("{} crf {}", self.codec, self.crf)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioEncoderConfig {
    pub codec: String,
    pub bitrate: String,
    pub channels: u32,
    pub sample_rate: u32,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        Self {
            codec: "aac".into(),
            bitrate: "192k".into(),
            channels: 2,
            sample_rate: 48000,
        }
    }
}

pub const AUDIO_CODECS: &[&str] = &[
    "aac",
    "libopus",
    "libmp3lame",
    "libvorbis",
    "flac",
    "pcm_s16le",
    "copy",
];

impl AudioEncoderConfig {
    pub fn args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["-c:a".into(), self.codec.clone()];
        if self.codec == "copy" {
            return a;
        }
        if !self.bitrate.trim().is_empty()
            && !matches!(self.codec.as_str(), "flac" | "pcm_s16le" | "pcm_s24le")
        {
            a.push("-b:a".into());
            a.push(self.bitrate.trim().to_string());
        }
        if self.channels > 0 {
            a.push("-ac".into());
            a.push(self.channels.to_string());
        }
        if self.sample_rate > 0 {
            a.push("-ar".into());
            a.push(self.sample_rate.to_string());
        }
        a
    }

    pub fn summary(&self) -> String {
        if self.codec == "copy" {
            return "copy".into();
        }
        format!("{} {}", self.codec, self.bitrate)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PackageConfig {
    pub container: String,
    pub movflags_faststart: bool,
    pub shortest: bool,
    pub metadata: Vec<MetaEntry>,
    pub name_template: String,
    /// Empty = write next to the source file.
    pub output_dir: String,
    /// Extra raw ffmpeg arguments appended before the output path.
    pub extra_args: String,
}

impl Default for PackageConfig {
    fn default() -> Self {
        Self {
            container: "mp4".into(),
            movflags_faststart: true,
            shortest: false,
            metadata: Vec::new(),
            name_template: "{dir}/{stem}_fflow.{ext}".into(),
            output_dir: String::new(),
            extra_args: String::new(),
        }
    }
}

pub const CONTAINERS: &[&str] = &["mp4", "mkv", "mov", "webm", "mp3", "m4a", "wav", "flac", "ogg"];

impl PackageConfig {
    pub fn ext(&self) -> &str {
        match self.container.as_str() {
            "mkv" => "mkv",
            "mov" => "mov",
            "webm" => "webm",
            "mp3" => "mp3",
            "m4a" => "m4a",
            "wav" => "wav",
            "flac" => "flac",
            "ogg" => "ogg",
            _ => "mp4",
        }
    }

    /// ffmpeg muxer name for `-f` (the container extension is not always a
    /// valid muxer name, e.g. `mkv` → `matroska`).
    pub fn muxer(&self) -> &'static str {
        match self.container.as_str() {
            "mkv" => "matroska",
            "m4a" => "ipod",
            "mov" => "mov",
            "webm" => "webm",
            "mp3" => "mp3",
            "wav" => "wav",
            "flac" => "flac",
            "ogg" => "ogg",
            _ => "mp4",
        }
    }

    /// ffmpeg output format + packaging flags (everything before the path).
    pub fn args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["-f".into(), self.muxer().to_string()];
        if self.movflags_faststart && matches!(self.container.as_str(), "mp4" | "mov" | "m4a") {
            a.push("-movflags".into());
            a.push("+faststart".into());
        }
        if self.shortest {
            a.push("-shortest".into());
        }
        for m in &self.metadata {
            if !m.key.trim().is_empty() {
                a.push("-metadata".into());
                a.push(format!("{}={}", m.key.trim(), m.value));
            }
        }
        for extra in self.extra_args.split_whitespace() {
            a.push(extra.to_string());
        }
        a
    }

    /// Containers that can carry subtitle streams at all.
    pub fn supports_subtitles(&self) -> bool {
        matches!(self.container.as_str(), "mp4" | "mov" | "m4a" | "mkv")
    }

    /// Subtitle codec to use for this container. MP4 cannot hold `subrip`;
    /// `mov_text` is its native format. Matroska takes anything.
    pub fn subtitle_codec(&self) -> &'static str {
        if self.container == "mkv" {
            "copy"
        } else {
            "mov_text"
        }
    }

    pub fn summary(&self) -> String {
        format!(".{} container", self.ext())
    }
}

// ---------------------------------------------------------------------------
// The config union
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum NodeConfig {
    Source(SourceConfig),
    Demux(DemuxConfig),

    Scale(ScaleConfig),
    Pad(PadConfig),
    Crop(CropConfig),
    Rotate(RotateConfig),
    Fps(FpsConfig),
    Color(ColorConfig),
    Lut(LutConfig),
    Sharpen(SharpenConfig),
    Denoise(DenoiseConfig),
    Overlay(OverlayConfig),

    Volume(VolumeConfig),
    Loudness(LoudnessConfig),
    Band(BandConfig),
    Tempo(TempoConfig),
    Channels(ChannelsConfig),
    Resample(ResampleConfig),

    RawFilter(RawFilterConfig),

    Condition(ConditionConfig),

    VideoEncoder(VideoEncoderConfig),
    AudioEncoder(AudioEncoderConfig),
    Package(PackageConfig),
}

impl NodeConfig {
    pub fn default_for(kind: NodeKind) -> Self {
        match kind {
            NodeKind::Source => NodeConfig::Source(SourceConfig::default()),
            NodeKind::Demux => NodeConfig::Demux(DemuxConfig::default()),
            NodeKind::Scale => NodeConfig::Scale(ScaleConfig::default()),
            NodeKind::Pad => NodeConfig::Pad(PadConfig::default()),
            NodeKind::Crop => NodeConfig::Crop(CropConfig::default()),
            NodeKind::Rotate => NodeConfig::Rotate(RotateConfig::default()),
            NodeKind::Fps => NodeConfig::Fps(FpsConfig::default()),
            NodeKind::Color => NodeConfig::Color(ColorConfig::default()),
            NodeKind::Lut => NodeConfig::Lut(LutConfig::default()),
            NodeKind::Sharpen => NodeConfig::Sharpen(SharpenConfig::default()),
            NodeKind::Denoise => NodeConfig::Denoise(DenoiseConfig::default()),
            NodeKind::Overlay => NodeConfig::Overlay(OverlayConfig::default()),
            NodeKind::Volume => NodeConfig::Volume(VolumeConfig::default()),
            NodeKind::Loudness => NodeConfig::Loudness(LoudnessConfig::default()),
            NodeKind::Band => NodeConfig::Band(BandConfig::default()),
            NodeKind::Tempo => NodeConfig::Tempo(TempoConfig::default()),
            NodeKind::Channels => NodeConfig::Channels(ChannelsConfig::default()),
            NodeKind::Resample => NodeConfig::Resample(ResampleConfig::default()),
            NodeKind::RawFilter => NodeConfig::RawFilter(RawFilterConfig::default()),
            NodeKind::Condition => NodeConfig::Condition(ConditionConfig::default()),
            NodeKind::VideoEncoder => NodeConfig::VideoEncoder(VideoEncoderConfig::default()),
            NodeKind::AudioEncoder => NodeConfig::AudioEncoder(AudioEncoderConfig::default()),
            NodeKind::Package => NodeConfig::Package(PackageConfig::default()),
        }
    }

    pub fn kind(&self) -> NodeKind {
        match self {
            NodeConfig::Source(_) => NodeKind::Source,
            NodeConfig::Demux(_) => NodeKind::Demux,
            NodeConfig::Scale(_) => NodeKind::Scale,
            NodeConfig::Pad(_) => NodeKind::Pad,
            NodeConfig::Crop(_) => NodeKind::Crop,
            NodeConfig::Rotate(_) => NodeKind::Rotate,
            NodeConfig::Fps(_) => NodeKind::Fps,
            NodeConfig::Color(_) => NodeKind::Color,
            NodeConfig::Lut(_) => NodeKind::Lut,
            NodeConfig::Sharpen(_) => NodeKind::Sharpen,
            NodeConfig::Denoise(_) => NodeKind::Denoise,
            NodeConfig::Overlay(_) => NodeKind::Overlay,
            NodeConfig::Volume(_) => NodeKind::Volume,
            NodeConfig::Loudness(_) => NodeKind::Loudness,
            NodeConfig::Band(_) => NodeKind::Band,
            NodeConfig::Tempo(_) => NodeKind::Tempo,
            NodeConfig::Channels(_) => NodeKind::Channels,
            NodeConfig::Resample(_) => NodeKind::Resample,
            NodeConfig::RawFilter(_) => NodeKind::RawFilter,
            NodeConfig::Condition(_) => NodeKind::Condition,
            NodeConfig::VideoEncoder(_) => NodeKind::VideoEncoder,
            NodeConfig::AudioEncoder(_) => NodeKind::AudioEncoder,
            NodeConfig::Package(_) => NodeKind::Package,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            NodeConfig::Source(c) => serde_json::to_value(c),
            NodeConfig::Demux(c) => serde_json::to_value(c),
            NodeConfig::Scale(c) => serde_json::to_value(c),
            NodeConfig::Pad(c) => serde_json::to_value(c),
            NodeConfig::Crop(c) => serde_json::to_value(c),
            NodeConfig::Rotate(c) => serde_json::to_value(c),
            NodeConfig::Fps(c) => serde_json::to_value(c),
            NodeConfig::Color(c) => serde_json::to_value(c),
            NodeConfig::Lut(c) => serde_json::to_value(c),
            NodeConfig::Sharpen(c) => serde_json::to_value(c),
            NodeConfig::Denoise(c) => serde_json::to_value(c),
            NodeConfig::Overlay(c) => serde_json::to_value(c),
            NodeConfig::Volume(c) => serde_json::to_value(c),
            NodeConfig::Loudness(c) => serde_json::to_value(c),
            NodeConfig::Band(c) => serde_json::to_value(c),
            NodeConfig::Tempo(c) => serde_json::to_value(c),
            NodeConfig::Channels(c) => serde_json::to_value(c),
            NodeConfig::Resample(c) => serde_json::to_value(c),
            NodeConfig::RawFilter(c) => serde_json::to_value(c),
            NodeConfig::Condition(c) => serde_json::to_value(c),
            NodeConfig::VideoEncoder(c) => serde_json::to_value(c),
            NodeConfig::AudioEncoder(c) => serde_json::to_value(c),
            NodeConfig::Package(c) => serde_json::to_value(c),
        }
        .unwrap_or(Value::Null)
    }

    pub fn from_value(kind: NodeKind, v: Value) -> anyhow::Result<Self> {
        if v.is_null() {
            return Ok(kind.default_config());
        }
        Ok(match kind {
            NodeKind::Source => NodeConfig::Source(serde_json::from_value(v)?),
            NodeKind::Demux => NodeConfig::Demux(serde_json::from_value(v)?),
            NodeKind::Scale => NodeConfig::Scale(serde_json::from_value(v)?),
            NodeKind::Pad => NodeConfig::Pad(serde_json::from_value(v)?),
            NodeKind::Crop => NodeConfig::Crop(serde_json::from_value(v)?),
            NodeKind::Rotate => NodeConfig::Rotate(serde_json::from_value(v)?),
            NodeKind::Fps => NodeConfig::Fps(serde_json::from_value(v)?),
            NodeKind::Color => NodeConfig::Color(serde_json::from_value(v)?),
            NodeKind::Lut => NodeConfig::Lut(serde_json::from_value(v)?),
            NodeKind::Sharpen => NodeConfig::Sharpen(serde_json::from_value(v)?),
            NodeKind::Denoise => NodeConfig::Denoise(serde_json::from_value(v)?),
            NodeKind::Overlay => NodeConfig::Overlay(serde_json::from_value(v)?),
            NodeKind::Volume => NodeConfig::Volume(serde_json::from_value(v)?),
            NodeKind::Loudness => NodeConfig::Loudness(serde_json::from_value(v)?),
            NodeKind::Band => NodeConfig::Band(serde_json::from_value(v)?),
            NodeKind::Tempo => NodeConfig::Tempo(serde_json::from_value(v)?),
            NodeKind::Channels => NodeConfig::Channels(serde_json::from_value(v)?),
            NodeKind::Resample => NodeConfig::Resample(serde_json::from_value(v)?),
            NodeKind::RawFilter => NodeConfig::RawFilter(serde_json::from_value(v)?),
            NodeKind::Condition => NodeConfig::Condition(serde_json::from_value(v)?),
            NodeKind::VideoEncoder => NodeConfig::VideoEncoder(serde_json::from_value(v)?),
            NodeKind::AudioEncoder => NodeConfig::AudioEncoder(serde_json::from_value(v)?),
            NodeKind::Package => NodeConfig::Package(serde_json::from_value(v)?),
        })
    }

    /// One-line description drawn under the node title.
    pub fn summary(&self) -> String {
        match self {
            NodeConfig::Source(c) => {
                let n = c.paths.len();
                if n == 0 {
                    "⚠ no path selected".into()
                } else if n == 1 {
                    crate::source::short_name(std::path::Path::new(&c.paths[0]))
                } else {
                    format!("{} paths", n)
                }
            }
            NodeConfig::Demux(c) => c.summary(),
            NodeConfig::Scale(c) => c.summary(),
            NodeConfig::Pad(c) => c.summary(),
            NodeConfig::Crop(c) => c.summary(),
            NodeConfig::Rotate(c) => c.summary(),
            NodeConfig::Fps(c) => c.summary(),
            NodeConfig::Color(c) => c.summary(),
            NodeConfig::Lut(c) => c.summary(),
            NodeConfig::Sharpen(c) => c.summary(),
            NodeConfig::Denoise(c) => c.summary(),
            NodeConfig::Overlay(c) => c.summary(),
            NodeConfig::Volume(c) => c.summary(),
            NodeConfig::Loudness(c) => c.summary(),
            NodeConfig::Band(c) => c.summary(),
            NodeConfig::Tempo(c) => c.summary(),
            NodeConfig::Channels(c) => c.summary(),
            NodeConfig::Resample(c) => c.summary(),
            NodeConfig::RawFilter(c) => c.summary(),
            NodeConfig::Condition(c) => c.summary(),
            NodeConfig::VideoEncoder(c) => c.summary(),
            NodeConfig::AudioEncoder(c) => c.summary(),
            NodeConfig::Package(c) => c.summary(),
        }
    }

    /// Human readable validation problems (empty = fine).
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        match self {
            NodeConfig::Source(c) => {
                if c.paths.is_empty() {
                    out.push("Select a root folder or drop files onto the canvas.".into());
                }
                for p in &c.paths {
                    if !std::path::Path::new(p).exists() {
                        out.push(format!("Path does not exist: {}", p));
                    }
                }
            }
            NodeConfig::Condition(c) => {
                // No rules means "always pass" — harmless, not a problem.
                for r in &c.rules {
                    if r.field == CondField::Tag && r.key.trim().is_empty() {
                        out.push("A metadata rule needs a key (e.g. comment, artist).".into());
                    }
                }
            }
            NodeConfig::Package(c) => {
                if c.name_template.trim().is_empty() {
                    out.push("Name template is empty.".into());
                }
            }
            other => {
                // Every filter card validates through the same call the
                // compiler makes, so the inspector can never disagree with
                // what will actually run.
                if let Err(e) = other.filters() {
                    out.push(e.to_string());
                }
            }
        }
        out
    }

    /// Filters this card contributes to a **video** branch, in order.
    ///
    /// Empty means the card is a passthrough (neutral colour, 0 dB gain, …).
    pub fn video_filters(&self) -> anyhow::Result<Vec<String>> {
        Ok(match self {
            NodeConfig::Scale(c) => c.filters()?,
            NodeConfig::Pad(c) => c.filters()?,
            NodeConfig::Crop(c) => c.filters()?,
            NodeConfig::Rotate(c) => c.filters()?,
            NodeConfig::Fps(c) => c.filters()?,
            NodeConfig::Color(c) => c.filters()?,
            NodeConfig::Lut(c) => c.filters()?,
            NodeConfig::Sharpen(c) => c.filters()?,
            NodeConfig::Denoise(c) => c.filters()?,
            NodeConfig::Overlay(c) => c.filters()?,
            NodeConfig::RawFilter(c) => c.filters()?,
            _ => Vec::new(),
        })
    }

    /// Filters this card contributes to an **audio** branch, in order.
    pub fn audio_filters(&self) -> anyhow::Result<Vec<String>> {
        Ok(match self {
            NodeConfig::Volume(c) => c.filters()?,
            NodeConfig::Loudness(c) => c.filters()?,
            NodeConfig::Band(c) => c.filters()?,
            NodeConfig::Tempo(c) => c.filters()?,
            NodeConfig::Channels(c) => c.filters()?,
            NodeConfig::Resample(c) => c.filters()?,
            NodeConfig::RawFilter(c) => c.filters()?,
            _ => Vec::new(),
        })
    }

    /// Either branch — used by `problems()`.
    fn filters(&self) -> anyhow::Result<Vec<String>> {
        match self {
            NodeConfig::Volume(c) => c.filters(),
            NodeConfig::Loudness(c) => c.filters(),
            NodeConfig::Band(c) => c.filters(),
            NodeConfig::Tempo(c) => c.filters(),
            NodeConfig::Channels(c) => c.filters(),
            NodeConfig::Resample(c) => c.filters(),
            _ => self.video_filters(),
        }
    }

    /// `(image, x, y, scale)` when this card overlays a second input.
    pub fn overlay(&self) -> Option<(String, String, String, f32)> {
        match self {
            NodeConfig::Overlay(c) => c.spec(),
            _ => None,
        }
    }

}

// ---------------------------------------------------------------------------
// Property inspector forms
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct InspectorOutcome {
    pub changed: bool,
    /// The user asked to re-probe the first file of this node (Source).
    pub request_probe: bool,
    /// The user asked for a fresh encoder detection run.
    pub redetect: bool,
}

const WARN: egui::Color32 = egui::Color32::from_rgb(220, 160, 60);
const OK: egui::Color32 = egui::Color32::from_rgb(110, 200, 130);
const ERR: egui::Color32 = egui::Color32::from_rgb(225, 120, 110);

fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String, changed: &mut bool) {
    crate::theme::field(ui, label, |ui| {
        if ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY)).changed() {
            *changed = true;
        }
    });
}

fn number_row_f32(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f32,
    changed: &mut bool,
) {
    crate::theme::field(ui, label, |ui| {
        if ui
            .add(egui::DragValue::new(value).speed(speed).range(range))
            .changed()
        {
            *changed = true;
        }
    });
}

fn number_row_u32(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    changed: &mut bool,
) {
    crate::theme::field(ui, label, |ui| {
        if ui.add(egui::DragValue::new(value).range(range)).changed() {
            *changed = true;
        }
    });
}

fn combo_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: &[&str],
    changed: &mut bool,
) {
    crate::theme::field(ui, label, |ui| {
        egui::ComboBox::from_id_salt((label, "combo"))
            .selected_text(value.clone())
            .width(ui.available_width().min(180.0))
            .show_ui(ui, |ui| {
                for opt in options {
                    if ui.selectable_label(value == opt, *opt).clicked() {
                        *value = (*opt).to_string();
                        *changed = true;
                    }
                }
            });
    });
}


// ---------------------------------------------------------------------------
// Inspector forms for the chain nodes
// ---------------------------------------------------------------------------

/// Lay `add` out in the rest of the current row, wrapping when it runs out of
/// width.
///
/// `ui.horizontal_wrapped` cannot be used directly here: inside a `field` row
/// the child `Ui` already starts after the label gutter, and egui would
/// subtract that offset a second time, wrapping far too early.
fn wrap_row(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let size = egui::vec2(ui.available_width().max(60.0), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
        |ui| add(ui),
    );
}

/// Segmented control bound to an enum.
///
/// The caption sits above the buttons rather than in the label gutter: four
/// options plus a 92 px gutter do not fit in a narrow inspector panel, and
/// wrapping a segmented control mid-row looks broken.
fn segments<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &'static str)],
    changed: &mut bool,
) {
    if !label.is_empty() {
        ui.label(
            egui::RichText::new(label)
                .size(crate::theme::CAPTION)
                .color(crate::theme::p().faint),
        );
    }
    wrap_row(ui, |ui| {
        ui.spacing_mut().button_padding.x = 7.0;
        ui.spacing_mut().item_spacing.x = 4.0;
        for (v, text) in options {
            if ui.selectable_label(*value == *v, *text).clicked() {
                *value = *v;
                *changed = true;
            }
        }
    });
}

/// Text field with a "…" file picker next to it.
fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    filter: (&str, &[&str]),
    changed: &mut bool,
) {
    crate::theme::field(ui, label, |ui| {
        let w = (ui.available_width() - 34.0).max(60.0);
        if ui
            .add(egui::TextEdit::singleline(value).desired_width(w))
            .changed()
        {
            *changed = true;
        }
        if ui.small_button("…").clicked() {
            if let Some(f) = rfd::FileDialog::new()
                .add_filter(filter.0, filter.1)
                .pick_file()
            {
                *value = f.to_string_lossy().to_string();
                *changed = true;
            }
        }
    });
}

fn condition_form(ui: &mut egui::Ui, c: &mut ConditionConfig, changed: &mut bool) {
    text_row(ui, "Name", &mut c.label, changed);
    segments(
        ui,
        "Match",
        &mut c.match_all,
        &[(true, "all"), (false, "any")],
        changed,
    );
    ui.label(
        egui::RichText::new(
            "checks the file arriving on `in`: matches leave through `pass`, \
             the rest through `fail`. Leave a side unconnected to skip those files.",
        )
        .small()
        .weak(),
    );
    ui.add_space(6.0);

    let mut remove: Option<usize> = None;
    for (i, rule) in c.rules.iter_mut().enumerate() {
        egui::Frame::none()
            .fill(crate::theme::p().surface)
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("cond-field", i))
                        .selected_text(rule.field.label())
                        .width(118.0)
                        .show_ui(ui, |ui| {
                            for f in CondField::ALL {
                                if ui.selectable_label(rule.field == f, f.label()).clicked() {
                                    rule.field = f;
                                    if f.is_number() && !rule.op.is_numeric() {
                                        rule.op = CondOp::Equals;
                                    } else if !f.is_number() && rule.op.is_numeric() {
                                        rule.op = CondOp::Contains;
                                    }
                                    *changed = true;
                                }
                            }
                        });
                    if ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    let ops: &[CondOp] = if rule.field.is_number() {
                        &CondOp::NUMBER
                    } else {
                        &CondOp::TEXT
                    };
                    egui::ComboBox::from_id_salt(("cond-op", i))
                        .selected_text(rule.op.label())
                        .width(118.0)
                        .show_ui(ui, |ui| {
                            for op in ops {
                                if ui.selectable_label(rule.op == *op, op.label()).clicked() {
                                    rule.op = *op;
                                    *changed = true;
                                }
                            }
                        });
                    let hint = if rule.field.is_number() { "0" } else { "value" };
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut rule.value)
                                .hint_text(hint)
                                .desired_width(96.0),
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                });
                if rule.field == CondField::Tag {
                    ui.add_space(3.0);
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut rule.key)
                                .hint_text("metadata key, e.g. artist")
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                }
            });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        c.rules.remove(i);
        *changed = true;
    }
    if ui.button("+ rule").clicked() {
        c.rules.push(CondRule {
            field: CondField::FileName,
            op: CondOp::Contains,
            value: String::new(),
            key: String::new(),
        });
        *changed = true;
    }
    if c.rules.is_empty() {
        ui.label(
            egui::RichText::new("no rules — every file takes the pass branch")
                .small()
                .weak(),
        );
    }
}

/// Segmented control for a [`TrackMode`].
fn track_mode_row(ui: &mut egui::Ui, label: &str, mode: &mut TrackMode, changed: &mut bool) {
    // Same stacked layout as `segments`: four options do not fit next to a
    // 92 px label gutter in a 360 px panel.
    segments(
        ui,
        label,
        mode,
        &TrackMode::ALL.map(|m| (m, m.label())),
        changed,
    );
}

/// One checkbox per probed stream.
fn track_list(
    ui: &mut egui::Ui,
    streams: &[crate::probe::StreamInfo],
    video: bool,
    selected: &mut Vec<u32>,
    changed: &mut bool,
) {
    for s in streams {
        let mut on = selected.contains(&s.ordinal);
        let text = format!("#{}  {}", s.ordinal, s.describe(video));
        if ui
            .checkbox(&mut on, egui::RichText::new(text).small())
            .changed()
        {
            if on {
                selected.push(s.ordinal);
                selected.sort_unstable();
                selected.dedup();
            } else {
                selected.retain(|t| *t != s.ordinal);
            }
            *changed = true;
        }
    }
}

/// Fallback editor used when nothing has been probed yet.
fn ordinals_row(ui: &mut egui::Ui, label: &str, value: &mut Vec<u32>, changed: &mut bool) {
    crate::theme::field(ui, label, |ui| {
        let mut text = value
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",");
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(ui.available_width().min(150.0))
                    .hint_text("0,1"),
            )
            .changed()
        {
            *value = text
                .split(',')
                .filter_map(|t| t.trim().parse::<u32>().ok())
                .collect();
            *changed = true;
        }
    });
}

/// Render the property form for a node config.
///
/// `probe` is the ffprobe result of the file shown in the preview panel (used
/// for the Demux track picker); `reg` is the encoder registry (used to grey out
/// unavailable encoders and show the hardware substitution).
pub fn inspector_ui(
    cfg: &mut NodeConfig,
    ui: &mut egui::Ui,
    probe: Option<&crate::probe::ProbeInfo>,
    reg: Option<&crate::encoders::EncoderRegistry>,
) -> InspectorOutcome {
    let mut out = InspectorOutcome::default();
    let changed = &mut out.changed;

    match cfg {
        NodeConfig::Source(c) => {
            ui.label("Folders / files");
            let mut remove: Option<usize> = None;
            for (i, p) in c.paths.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.add(egui::TextEdit::singleline(p).desired_width(230.0)).changed() {
                        *changed = true;
                    }
                    if ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                c.paths.remove(i);
                *changed = true;
            }
            ui.horizontal(|ui| {
                if ui.button("+ Folder").clicked() {
                    // Start where the node already points, so adding a second
                    // folder does not begin from the last place the OS dialog
                    // happened to be.
                    let start = c
                        .paths
                        .last()
                        .map(std::path::PathBuf::from)
                        .filter(|p| p.is_dir());
                    if let Some(dir) = crate::source::pick_folder(start.as_deref()) {
                        c.paths.push(dir.to_string_lossy().to_string());
                        *changed = true;
                    }
                }
                if ui.button("+ Files").clicked() {
                    if let Some(files) = rfd::FileDialog::new()
                        .set_title("Choose media files")
                        .add_filter("Media", crate::source::EXT_LIST)
                        .pick_files()
                    {
                        for f in files {
                            c.paths.push(f.to_string_lossy().to_string());
                        }
                        *changed = true;
                    }
                }
            });
            ui.add_space(4.0);
            if ui.checkbox(&mut c.recursive, "Recursive (walkdir)").changed() {
                *changed = true;
            }
            if ui
                .checkbox(&mut c.skip_own_outputs, "Skip previous fflow outputs")
                .changed()
            {
                *changed = true;
            }
            text_row(ui, "Extensions", &mut c.extensions, changed);
            ui.label(
                egui::RichText::new("comma separated, empty = every file")
                    .weak()
                    .small(),
            );
            ui.add_space(4.0);
            if ui.button("🔍 Probe first file").clicked() {
                out.request_probe = true;
            }
        }
        NodeConfig::Demux(c) => {
            match probe {
                Some(p) => {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} · {:.2}s",
                            p.streams_summary(),
                            p.resolution(),
                            p.duration
                        ))
                        .small()
                        .weak(),
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new(
                            "Probe a file to see its tracks. Without a probe, track lists \
                             fall back to plain ordinals.",
                        )
                        .small()
                        .weak(),
                    );
                }
            }
            ui.add_space(6.0);

            // ---- video ----
            track_mode_row(ui, "Video tracks", &mut c.video_mode, changed);
            if c.video_mode == TrackMode::Select {
                match probe.filter(|p| !p.video_streams.is_empty()) {
                    Some(p) => track_list(ui, &p.video_streams, true, &mut c.video_tracks, changed),
                    None => ordinals_row(ui, "  ordinals", &mut c.video_tracks, changed),
                }
            }
            ui.add_space(4.0);

            // ---- audio ----
            track_mode_row(ui, "Audio tracks", &mut c.audio_mode, changed);
            if c.audio_mode == TrackMode::Select {
                match probe.filter(|p| !p.audio_streams.is_empty()) {
                    Some(p) => track_list(ui, &p.audio_streams, false, &mut c.audio_tracks, changed),
                    None => ordinals_row(ui, "  ordinals", &mut c.audio_tracks, changed),
                }
            }
            ui.add_space(4.0);

            // ---- subtitles ----
            track_mode_row(ui, "Subtitles", &mut c.subtitle_mode, changed);
            if c.subtitle_mode == TrackMode::Select {
                match probe.filter(|p| !p.subtitle_streams.is_empty()) {
                    Some(p) => track_list(ui, &p.subtitle_streams, false, &mut c.subtitle_tracks, changed),
                    None => ordinals_row(ui, "  ordinals", &mut c.subtitle_tracks, changed),
                }
            }
            if c.subtitle_mode != TrackMode::Off {
                ui.label(
                    egui::RichText::new(
                        "Subtitles are copied as-is (mov_text for mp4/mov). Containers \
                         without subtitle support ignore this.",
                    )
                    .small()
                    .weak(),
                );
            }
        }
        NodeConfig::Scale(c) => scale_form(ui, c, changed),
        NodeConfig::Pad(c) => pad_form(ui, c, changed),
        NodeConfig::Crop(c) => crop_form(ui, c, changed),
        NodeConfig::Rotate(c) => rotate_form(ui, c, changed),
        NodeConfig::Fps(c) => fps_form(ui, c, changed),
        NodeConfig::Color(c) => color_form(ui, c, changed),
        NodeConfig::Lut(c) => lut_form(ui, c, changed),
        NodeConfig::Sharpen(c) => sharpen_form(ui, c, changed),
        NodeConfig::Denoise(c) => denoise_form(ui, c, changed),
        NodeConfig::Overlay(c) => overlay_form(ui, c, changed),
        NodeConfig::Volume(c) => volume_form(ui, c, changed),
        NodeConfig::Loudness(c) => loudness_form(ui, c, changed),
        NodeConfig::Band(c) => band_form(ui, c, changed),
        NodeConfig::Tempo(c) => tempo_form(ui, c, changed),
        NodeConfig::Channels(c) => channels_form(ui, c, changed),
        NodeConfig::Resample(c) => resample_form(ui, c, changed),
        NodeConfig::RawFilter(c) => raw_form(ui, c, changed),
        NodeConfig::Condition(c) => condition_form(ui, c, changed),
        NodeConfig::VideoEncoder(c) => {
            combo_row(ui, "Codec", &mut c.codec, VIDEO_CODECS, changed);

            // Availability / hardware substitution feedback.
            if c.codec != "copy" {
                match reg {
                    Some(r) => {
                        let (effective, note) = c.effective_codec(Some(r));
                        if effective != c.codec {
                            ui.colored_label(
                                OK,
                                egui::RichText::new(format!("› runs as {}", effective)).small(),
                            );
                        } else if !r.is_usable(&c.codec) {
                            ui.colored_label(
                                ERR,
                                egui::RichText::new(format!(
                                    "⚠ {} is not available in this ffmpeg build",
                                    c.codec
                                ))
                                .small(),
                            );
                        }
                        if let Some(n) = note {
                            ui.label(egui::RichText::new(n).small().weak());
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(if crate::encoders::is_detecting() {
                                "detecting encoders…"
                            } else {
                                "encoder list unknown"
                            })
                            .small()
                            .weak(),
                        );
                    }
                }
                if ui
                    .checkbox(&mut c.prefer_hardware, "Prefer hardware encoder")
                    .on_hover_text(
                        "Substitutes a verified hardware encoder (NVENC / QSV / AMF / \
                         VideoToolbox) for the chosen software codec. Falls back when the \
                         GPU cannot encode.",
                    )
                    .changed()
                {
                    *changed = true;
                }
            }

            if c.codec != "copy" {
                if ui.checkbox(&mut c.use_bitrate, "Target bitrate instead of CRF").changed() {
                    *changed = true;
                }
                if c.use_bitrate {
                    text_row(ui, "Bitrate", &mut c.bitrate, changed);
                } else {
                    number_row_u32(ui, "CRF", &mut c.crf, 0..=63, changed);
                }
                let (effective, _) = c.effective_codec(reg);
                if matches!(
                    crate::encoders::hw_kind(&effective),
                    crate::encoders::HwKind::Software
                ) || crate::encoders::hw_kind(&effective) == crate::encoders::HwKind::Qsv
                {
                    combo_row(ui, "Preset", &mut c.preset, PRESETS, changed);
                }
                text_row(ui, "Pixel format", &mut c.pix_fmt, changed);
            } else {
                ui.label(
                    egui::RichText::new("stream copy: no re-encode")
                        .weak()
                        .small(),
                );
            }

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("↻ detect encoders").clicked() {
                    out.redetect = true;
                }
                match reg {
                    Some(r) => {
                        ui.label(egui::RichText::new(r.summary()).small().weak());
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(if crate::encoders::is_detecting() {
                                "detecting…"
                            } else {
                                "not detected yet"
                            })
                            .small()
                            .weak(),
                        );
                    }
                }
            });
        }
        NodeConfig::AudioEncoder(c) => {
            combo_row(ui, "Codec", &mut c.codec, AUDIO_CODECS, changed);
            if c.codec != "copy" {
                text_row(ui, "Bitrate", &mut c.bitrate, changed);
                number_row_u32(ui, "Channels", &mut c.channels, 0..=8, changed);
                number_row_u32(ui, "Sample rate", &mut c.sample_rate, 0..=192000, changed);
            } else {
                ui.label(
                    egui::RichText::new("stream copy: no re-encode")
                        .weak()
                        .small(),
                );
            }
        }
        NodeConfig::Package(c) => {
            combo_row(ui, "Container", &mut c.container, CONTAINERS, changed);
            if ui.checkbox(&mut c.movflags_faststart, "movflags +faststart").changed() {
                *changed = true;
            }
            if ui.checkbox(&mut c.shortest, "-shortest").changed() {
                *changed = true;
            }
            text_row(ui, "Name template", &mut c.name_template, changed);
            ui.label(
                egui::RichText::new("tokens: {dir} {stem} {name} {ext} {index} {rel_dir}")
                    .size(crate::theme::CAPTION)
                    .color(crate::theme::p().faint),
            );
            crate::theme::field(ui, "Output dir", |ui| {
                if ui
                    .add(egui::TextEdit::singleline(&mut c.output_dir).desired_width(160.0))
                    .changed()
                {
                    *changed = true;
                }
                if ui.small_button("…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        c.output_dir = dir.to_string_lossy().to_string();
                        *changed = true;
                    }
                }
            });
            ui.add_space(4.0);
            ui.label("Metadata");
            let mut remove: Option<usize> = None;
            for (i, m) in c.metadata.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::TextEdit::singleline(&mut m.key).desired_width(90.0).hint_text("key"))
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut m.value)
                                .desired_width(130.0)
                                .hint_text("value"),
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                c.metadata.remove(i);
                *changed = true;
            }
            if ui.small_button("+ metadata").clicked() {
                c.metadata.push(MetaEntry {
                    key: "title".into(),
                    value: String::new(),
                });
                *changed = true;
            }
            ui.add_space(4.0);
            text_row(ui, "Extra args", &mut c.extra_args, changed);
        }
    }

    let problems = cfg.problems();
    if !problems.is_empty() {
        ui.add_space(6.0);
        for p in problems {
            ui.colored_label(WARN, format!("⚠ {}", p));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoders::EncoderRegistry;
    use crate::probe::{ProbeInfo, StreamInfo};

    fn probe(v: usize, a: usize, s: usize) -> ProbeInfo {
        ProbeInfo {
            has_video: v > 0,
            has_audio: a > 0,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            width: 1920,
            height: 1080,
            fps: 25.0,
            duration: 42.0,
            video_streams: (0..v)
                .map(|i| StreamInfo {
                    ordinal: i as u32,
                    codec: "h264".into(),
                    width: 1920,
                    height: 1080,
                    fps: 25.0,
                    ..Default::default()
                })
                .collect(),
            audio_streams: (0..a)
                .map(|i| StreamInfo {
                    ordinal: i as u32,
                    codec: "aac".into(),
                    channels: 2,
                    sample_rate: 48000,
                    language: if i == 0 { "jpn".into() } else { "eng".into() },
                    default: i == 0,
                    ..Default::default()
                })
                .collect(),
            subtitle_streams: (0..s)
                .map(|i| StreamInfo {
                    ordinal: i as u32,
                    codec: "subrip".into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn registry() -> EncoderRegistry {
        let mut reg = EncoderRegistry::default();
        reg.video.insert("libx264".into());
        reg.video.insert("h264_nvenc".into());
        reg.audio.insert("aac".into());
        reg.hw.insert("h264_nvenc".into(), true);
        reg
    }

    /// Drive one inspector frame; `probe`/`reg` mirror what the app passes in.
    fn render(cfg: &mut NodeConfig, probe: Option<&ProbeInfo>, reg: Option<&EncoderRegistry>) {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect =
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 1200.0)));
        for _ in 0..2 {
            let _ = ctx.run(input.clone(), |c| {
                egui::SidePanel::right("inspector-test").show(c, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let _ = inspector_ui(cfg, ui, probe, reg);
                    });
                });
            });
        }
    }

    #[test]
    fn demux_inspector_renders_every_track_mode() {
        let p = probe(2, 3, 2);
        let mut cfg = NodeConfig::Demux(DemuxConfig {
            audio_mode: TrackMode::Select,
            audio_tracks: vec![1, 2],
            subtitle_mode: TrackMode::All,
            ..Default::default()
        });
        for mode in TrackMode::ALL {
            if let NodeConfig::Demux(c) = &mut cfg {
                c.video_mode = mode;
                c.audio_mode = mode;
                c.subtitle_mode = mode;
            }
            // with probe data (checkbox list) and without (ordinal text field)
            render(&mut cfg, Some(&p), None);
            render(&mut cfg, None, None);
        }
    }

    #[test]
    fn encoder_inspector_shows_hardware_state() {
        let reg = registry();
        let mut cfg = NodeConfig::VideoEncoder(VideoEncoderConfig::default());
        // hardware substitution available
        render(&mut cfg, None, Some(&reg));
        // no registry yet (startup race)
        render(&mut cfg, None, None);
        // a codec this build does not have
        if let NodeConfig::VideoEncoder(c) = &mut cfg {
            c.codec = "libsvtav1".into();
        }
        render(&mut cfg, None, Some(&reg));
        // bitrate mode + copy
        if let NodeConfig::VideoEncoder(c) = &mut cfg {
            c.codec = "libx264".into();
            c.use_bitrate = true;
            c.prefer_hardware = false;
        }
        render(&mut cfg, None, Some(&reg));
        if let NodeConfig::VideoEncoder(c) = &mut cfg {
            c.codec = "copy".into();
        }
        render(&mut cfg, None, Some(&reg));
    }

    #[test]
    fn every_node_form_renders() {
        let p = probe(1, 2, 1);
        let reg = registry();
        for kind in NodeKind::ALL {
            let mut cfg = kind.default_config();
            render(&mut cfg, Some(&p), Some(&reg));
        }
    }

    #[test]
    fn track_mode_resolve_defaults() {
        // Defaults must reproduce the historical single-track behaviour.
        let d = DemuxConfig::default();
        assert_eq!(d.video_mode.resolve(&d.video_tracks, 2), vec![0]);
        assert_eq!(d.audio_mode.resolve(&d.audio_tracks, 3), vec![0]);
        assert!(d.subtitle_mode.resolve(&d.subtitle_tracks, 2).is_empty());
    }

    #[test]
    fn empty_config_json_uses_defaults() {
        // Old workflows stored `{}` for the Demux node; loading them must work.
        let cfg = NodeConfig::from_value(NodeKind::Demux, serde_json::json!({})).unwrap();
        match cfg {
            NodeConfig::Demux(c) => assert_eq!(c.video_mode, TrackMode::First),
            _ => panic!("wrong variant"),
        }
        // and a partial config keeps the given field, defaults the rest
        let cfg = NodeConfig::from_value(
            NodeKind::Demux,
            serde_json::json!({"audio_mode": "All"}),
        )
        .unwrap();
        match cfg {
            NodeConfig::Demux(c) => {
                assert_eq!(c.audio_mode, TrackMode::All);
                assert_eq!(c.video_mode, TrackMode::First);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn package_container_subtitle_support() {
        let mut p = PackageConfig::default();
        assert!(p.supports_subtitles());
        assert_eq!(p.subtitle_codec(), "mov_text");
        p.container = "mkv".into();
        assert!(p.supports_subtitles());
        assert_eq!(p.subtitle_codec(), "copy");
        p.container = "mp3".into();
        assert!(!p.supports_subtitles());
    }
    #[test]
    fn condition_rules_match_name_and_tags() {
        let info = probe(1, 1, 0);
        fn facts<'a>(name: &'a str, info: &'a ProbeInfo) -> FileFacts<'a> {
            FileFacts {
                path: std::path::Path::new(name),
                size: 12 * 1024 * 1024,
                info,
            }
        }

        let ext = ConditionConfig {
            rules: vec![CondRule {
                field: CondField::Extension,
                op: CondOp::Equals,
                value: "MP4".into(),
                key: String::new(),
            }],
            match_all: true,
            label: String::new(),
        };
        assert!(ext.matches(&facts("a.mp4", &info)));
        assert!(!ext.matches(&facts("a.mov", &info)));

        let glob = ConditionConfig {
            rules: vec![CondRule {
                field: CondField::FileName,
                op: CondOp::Glob,
                value: "intro*".into(),
                key: String::new(),
            }],
            ..Default::default()
        };
        assert!(glob.matches(&facts("intro_01.mkv", &info)));
        assert!(!glob.matches(&facts("outro_01.mkv", &info)));

        // numeric comparison on duration
        let long = ConditionConfig {
            rules: vec![CondRule {
                field: CondField::Duration,
                op: CondOp::Greater,
                value: "30".into(),
                key: String::new(),
            }],
            ..Default::default()
        };
        assert!(long.matches(&facts("a.mp4", &info))); // probe() reports 42s

        // metadata tag
        let mut tagged = probe(1, 1, 0);
        tagged.tags.insert("artist".into(), "ABBA".into());
        let tag = ConditionConfig {
            rules: vec![CondRule {
                field: CondField::Tag,
                op: CondOp::Contains,
                value: "abba".into(),
                key: "artist".into(),
            }],
            ..Default::default()
        };
        assert!(tag.matches(&FileFacts {
            path: std::path::Path::new("a.mp4"),
            size: 0,
            info: &tagged,
        }));

        // OR semantics
        let either = ConditionConfig {
            rules: vec![
                CondRule { field: CondField::Extension, op: CondOp::Equals, value: "mov".into(), key: String::new() },
                CondRule { field: CondField::FileName, op: CondOp::Contains, value: "intro".into(), key: String::new() },
            ],
            match_all: false,
            label: String::new(),
        };
        assert!(either.matches(&facts("intro.mp4", &info)));
        assert!(!either.matches(&facts("outro.mp4", &info)));
    }

    #[test]
    fn glob_matches_wildcards() {
        assert!(glob_match("*.mp4", "a.mp4"));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("*.mp4", "a.mp4.bak"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*b*c", "axxbyyc"));
        // the operator layer lower-cases both sides before matching
        assert!(text_matches(CondOp::Glob, "INTRO_01.MP4", "intro*"));
        assert!(text_matches(CondOp::Equals, "Clip.MP4", "clip.mp4"));
        assert!(!text_matches(CondOp::Equals, "clip.mp4", "clip"));
    }

    #[test]
    fn condition_node_reports_bad_rules() {
        let mut cfg = NodeConfig::Condition(ConditionConfig { rules: vec![], ..Default::default() });
        assert!(cfg.problems().is_empty(), "empty rules = always then");
        if let NodeConfig::Condition(c) = &mut cfg {
            c.rules.push(CondRule {
                field: CondField::Tag,
                op: CondOp::Contains,
                value: "x".into(),
                key: "  ".into(),
            });
        }
        assert!(cfg.problems().iter().any(|p| p.contains("needs a key")));
    }
}
