//! One card per filter.
//!
//! Each card is a single ffmpeg filter (or one tightly-coupled pair, like
//! `transpose` + `hflip`) with a handful of controls, so a chain of cards on
//! the canvas reads as the filtergraph it produces. The order the filters run
//! in comes from the wiring, not from a hard-coded list.

use serde::{Deserialize, Serialize};

use crate::tr;

use super::{number_row_f32, number_row_u32, path_row, segments, text_row, wrap_row};

// ---------------------------------------------------------------------------
// Shared enums
// ---------------------------------------------------------------------------

/// How the target resolution relates to the source aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScaleMode {
    /// `scale=w:h` verbatim — one axis may be `-2` to keep the ratio.
    #[default]
    Exact,
    /// Fit inside the box, keeping the ratio (may end up smaller on one axis).
    Fit,
    /// Cover the box, keeping the ratio, then crop the overflow away.
    Fill,
}

impl ScaleMode {
    pub const ALL: [ScaleMode; 3] = [ScaleMode::Exact, ScaleMode::Fit, ScaleMode::Fill];

    pub fn label(&self) -> &'static str {
        match self {
            ScaleMode::Exact => tr!("exact"),
            ScaleMode::Fit => tr!("fit"),
            ScaleMode::Fill => tr!("fill"),
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            ScaleMode::Exact => tr!("scale to these numbers; -2 keeps the aspect ratio"),
            ScaleMode::Fit => tr!("largest size inside the box, aspect preserved"),
            ScaleMode::Fill => tr!("cover the box, crop the overflow"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Rotate {
    #[default]
    None,
    Cw90,
    R180,
    Cw270,
}

impl Rotate {
    pub const ALL: [Rotate; 4] = [Rotate::None, Rotate::Cw90, Rotate::R180, Rotate::Cw270];

    pub fn label(&self) -> &'static str {
        match self {
            Rotate::None => tr!("none"),
            Rotate::Cw90 => "90°",
            Rotate::R180 => "180°",
            Rotate::Cw270 => "270°",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Flip {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

impl Flip {
    pub const ALL: [Flip; 4] = [Flip::None, Flip::Horizontal, Flip::Vertical, Flip::Both];

    pub fn label(&self) -> &'static str {
        match self {
            Flip::None => tr!("none"),
            Flip::Horizontal => tr!("h"),
            Flip::Vertical => tr!("v"),
            Flip::Both => tr!("both"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LutInterp {
    Nearest,
    Trilinear,
    #[default]
    Tetrahedral,
}

impl LutInterp {
    pub const ALL: [LutInterp; 3] = [
        LutInterp::Nearest,
        LutInterp::Trilinear,
        LutInterp::Tetrahedral,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            LutInterp::Nearest => tr!("nearest"),
            LutInterp::Trilinear => tr!("trilinear"),
            LutInterp::Tetrahedral => tr!("tetrahedral"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChannelMode {
    Mono,
    #[default]
    Stereo,
    FiveOne,
}

impl ChannelMode {
    pub const ALL: [ChannelMode; 3] =
        [ChannelMode::Mono, ChannelMode::Stereo, ChannelMode::FiveOne];

    pub fn label(&self) -> &'static str {
        match self {
            ChannelMode::Mono => tr!("mono"),
            ChannelMode::Stereo => tr!("stereo"),
            ChannelMode::FiveOne => "5.1",
        }
    }

    pub fn layout(&self) -> &'static str {
        match self {
            ChannelMode::Mono => tr!("mono"),
            ChannelMode::Stereo => tr!("stereo"),
            ChannelMode::FiveOne => "5.1",
        }
    }
}

/// Escape a path for use *inside* a filtergraph argument.
///
/// The filtergraph is a single argv element, but a value goes through **two**
/// parsers: the filtergraph parser splits filters on `,` and options on `:`,
/// then each filter parses its own option string with the same rules. A colon
/// therefore has to be escaped twice — `C:/a/b.cube` becomes `C\\:/a/b.cube`.
pub fn escape_filter_path(path: &str) -> String {
    path.replace('\\', "/")
        .replace(':', "\\\\:")
        .replace('\'', "\\\\'")
}

/// Trim a free-text numeric expression, falling back to `default`.
fn expr<'a>(s: &'a str, default: &'a str) -> &'a str {
    if s.trim().is_empty() {
        default
    } else {
        s.trim()
    }
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut)
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScaleConfig {
    pub width: String,
    pub height: String,
    pub mode: ScaleMode,
}

impl Default for ScaleConfig {
    fn default() -> Self {
        Self {
            width: "1920".into(),
            height: "1080".into(),
            mode: ScaleMode::Fit,
        }
    }
}

impl ScaleConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        let w = expr(&self.width, "-2");
        let h = expr(&self.height, "-2");
        if w == "-2" && h == "-2" {
            anyhow::bail!("scale needs at least a width or a height");
        }
        Ok(match self.mode {
            ScaleMode::Exact => vec![format!("scale={}:{}", w, h)],
            ScaleMode::Fit => vec![format!("scale={}:{}:force_original_aspect_ratio=decrease", w, h)],
            ScaleMode::Fill => {
                if w == "-2" || h == "-2" {
                    anyhow::bail!("fill needs both a width and a height");
                }
                vec![
                    format!("scale={}:{}:force_original_aspect_ratio=increase", w, h),
                    format!("crop={}:{}", w, h),
                ]
            }
        })
    }

    pub fn summary(&self) -> String {
        match self.filters() {
            Ok(_) => format!(
                "{}×{} {}",
                expr(&self.width, "auto"),
                expr(&self.height, "auto"),
                self.mode.label()
            ),
            Err(e) => format!("⚠ {}", e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PadConfig {
    pub width: String,
    pub height: String,
    pub x: String,
    pub y: String,
    pub color: String,
}

impl Default for PadConfig {
    fn default() -> Self {
        Self {
            width: "1920".into(),
            height: "1080".into(),
            x: "(ow-iw)/2".into(),
            y: "(oh-ih)/2".into(),
            color: "#000000".into(),
        }
    }
}

impl PadConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if self.width.trim().is_empty() || self.height.trim().is_empty() {
            anyhow::bail!("pad needs a width and a height");
        }
        Ok(vec![format!(
            "pad={}:{}:{}:{}:color={}",
            self.width.trim(),
            self.height.trim(),
            expr(&self.x, "(ow-iw)/2"),
            expr(&self.y, "(oh-ih)/2"),
            expr(&self.color, "black"),
        )])
    }

    pub fn summary(&self) -> String {
        match self.filters() {
            Ok(_) => format!("{}×{}", self.width.trim(), self.height.trim()),
            Err(e) => format!("⚠ {}", e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CropConfig {
    pub width: String,
    pub height: String,
    pub x: String,
    pub y: String,
}

impl Default for CropConfig {
    fn default() -> Self {
        Self {
            width: "iw".into(),
            height: "ih".into(),
            x: "(iw-ow)/2".into(),
            y: "(ih-oh)/2".into(),
        }
    }
}

impl CropConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if self.width.trim().is_empty() || self.height.trim().is_empty() {
            anyhow::bail!("crop needs a width and a height");
        }
        Ok(vec![format!(
            "crop={}:{}:{}:{}",
            self.width.trim(),
            self.height.trim(),
            expr(&self.x, "0"),
            expr(&self.y, "0"),
        )])
    }

    pub fn summary(&self) -> String {
        match self.filters() {
            Ok(_) => format!("{}×{}", self.width.trim(), self.height.trim()),
            Err(e) => format!("⚠ {}", e),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RotateConfig {
    pub rotate: Rotate,
    pub flip: Flip,
}

impl RotateConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        let mut out = Vec::new();
        match self.rotate {
            Rotate::None => {}
            Rotate::Cw90 => out.push("transpose=1".to_string()),
            Rotate::R180 => out.push("transpose=2,transpose=2".to_string()),
            Rotate::Cw270 => out.push("transpose=2".to_string()),
        }
        match self.flip {
            Flip::None => {}
            Flip::Horizontal => out.push("hflip".into()),
            Flip::Vertical => out.push("vflip".into()),
            Flip::Both => out.push("hflip,vflip".into()),
        }
        Ok(out)
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.rotate != Rotate::None {
            parts.push(self.rotate.label().to_string());
        }
        if self.flip != Flip::None {
            parts.push(tr!("flip {}", self.flip.label()));
        }
        if parts.is_empty() {
            "passthrough".into()
        } else {
            parts.join(" · ")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FpsConfig {
    pub fps: String,
}

impl Default for FpsConfig {
    fn default() -> Self {
        Self { fps: "30".into() }
    }
}

impl FpsConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        let fps: f64 = self
            .fps
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("fps must be a number"))?;
        if fps <= 0.0 || fps > 240.0 {
            anyhow::bail!("fps out of range (0, 240]");
        }
        Ok(vec![format!("fps={}", fps)])
    }

    pub fn summary(&self) -> String {
        match self.filters() {
            Ok(_) => format!("{} fps", self.fps.trim()),
            Err(e) => format!("⚠ {}", e),
        }
    }
}

// ---------------------------------------------------------------------------
// Look
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub gamma: f32,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
        }
    }
}

impl ColorConfig {
    pub fn is_neutral(&self) -> bool {
        self.brightness.abs() < 1e-4
            && (self.contrast - 1.0).abs() < 1e-4
            && (self.saturation - 1.0).abs() < 1e-4
            && (self.gamma - 1.0).abs() < 1e-4
    }

    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(-1.0..=1.0).contains(&self.brightness) {
            anyhow::bail!("brightness out of range [-1, 1]");
        }
        if !(0.0..=4.0).contains(&self.contrast) {
            anyhow::bail!("contrast out of range [0, 4]");
        }
        if !(0.0..=3.0).contains(&self.saturation) {
            anyhow::bail!("saturation out of range [0, 3]");
        }
        if !(0.1..=10.0).contains(&self.gamma) {
            anyhow::bail!("gamma out of range [0.1, 10]");
        }
        if self.is_neutral() {
            return Ok(Vec::new());
        }
        Ok(vec![format!(
            "eq=brightness={}:contrast={}:saturation={}:gamma={}",
            self.brightness, self.contrast, self.saturation, self.gamma
        )])
    }

    pub fn summary(&self) -> String {
        if self.is_neutral() {
            return tr!("neutral").into();
        }
        format!(
            "b {:+.2} · c {:.2} · s {:.2}",
            self.brightness, self.contrast, self.saturation
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LutConfig {
    pub path: String,
    pub interp: LutInterp,
}

impl Default for LutConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            interp: LutInterp::Tetrahedral,
        }
    }
}

impl LutConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        let path = self.path.trim();
        if path.is_empty() {
            anyhow::bail!("pick a .cube (or .3dl) file");
        }
        Ok(vec![format!(
            "lut3d=file={}:interp={}",
            escape_filter_path(path),
            self.interp.label()
        )])
    }

    pub fn summary(&self) -> String {
        if self.path.trim().is_empty() {
            return "⚠ no LUT file".into();
        }
        let name = std::path::Path::new(self.path.trim())
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.clone());
        format!("{} · {}", short(&name, 20), self.interp.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SharpenConfig {
    pub amount: f32,
}

impl Default for SharpenConfig {
    fn default() -> Self {
        Self { amount: 1.0 }
    }
}

impl SharpenConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(0.0..=3.0).contains(&self.amount) {
            anyhow::bail!("sharpen amount out of range [0, 3]");
        }
        if self.amount.abs() < 1e-4 {
            return Ok(Vec::new());
        }
        Ok(vec![format!("unsharp=5:5:{}:5:5:0", self.amount)])
    }

    pub fn summary(&self) -> String {
        if self.amount.abs() < 1e-4 {
            tr!("off").into()
        } else {
            format!("{:.2}", self.amount)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DenoiseConfig {
    pub strength: f32,
}

impl Default for DenoiseConfig {
    fn default() -> Self {
        Self { strength: 3.0 }
    }
}

impl DenoiseConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(0.0..=20.0).contains(&self.strength) {
            anyhow::bail!("denoise strength out of range [0, 20]");
        }
        if self.strength.abs() < 1e-4 {
            return Ok(Vec::new());
        }
        Ok(vec![format!("hqdn3d={}", self.strength)])
    }

    pub fn summary(&self) -> String {
        if self.strength.abs() < 1e-4 {
            tr!("off").into()
        } else {
            format!("{:.1}", self.strength)
        }
    }
}

/// The overlay filter needs a second input, so it is compiled separately from
/// the comma-joined chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub path: String,
    pub x: String,
    pub y: String,
    pub scale: f32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            x: "10".into(),
            y: "10".into(),
            scale: 1.0,
        }
    }
}

impl OverlayConfig {
    /// `(path, x, y, scale)` when the overlay is usable.
    pub fn spec(&self) -> Option<(String, String, String, f32)> {
        if self.path.trim().is_empty() {
            return None;
        }
        Some((
            self.path.trim().to_string(),
            expr(&self.x, "0").to_string(),
            expr(&self.y, "0").to_string(),
            self.scale,
        ))
    }

    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if self.path.trim().is_empty() {
            anyhow::bail!("pick an image to overlay");
        }
        if !(0.05..=4.0).contains(&self.scale) {
            anyhow::bail!("overlay scale out of range [0.05, 4]");
        }
        Ok(Vec::new())
    }

    pub fn summary(&self) -> String {
        if self.path.trim().is_empty() {
            return "⚠ no image".into();
        }
        let name = std::path::Path::new(self.path.trim())
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.clone());
        format!("{} @ {},{}", short(&name, 18), expr(&self.x, "0"), expr(&self.y, "0"))
    }
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VolumeConfig {
    /// Gain in dB; 0 is a no-op.
    pub db: f32,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self { db: 0.0 }
    }
}

impl VolumeConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(-60.0..=24.0).contains(&self.db) {
            anyhow::bail!("gain out of range [-60, 24] dB");
        }
        if self.db.abs() < 1e-3 {
            return Ok(Vec::new());
        }
        Ok(vec![format!("volume={}dB", self.db)])
    }

    pub fn summary(&self) -> String {
        if self.db.abs() < 1e-3 {
            "0.0 dB (no change)".into()
        } else {
            format!("{:+.1} dB", self.db)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoudnessConfig {
    /// Integrated loudness target in LUFS (typically -24 … -9).
    pub lufs: f32,
    /// True peak ceiling in dBTP.
    pub peak: f32,
}

impl Default for LoudnessConfig {
    fn default() -> Self {
        Self {
            lufs: -16.0,
            peak: -1.5,
        }
    }
}

impl LoudnessConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(-70.0..=-5.0).contains(&self.lufs) {
            anyhow::bail!("loudness target out of range [-70, -5] LUFS");
        }
        if !(-9.0..=0.0).contains(&self.peak) {
            anyhow::bail!("true peak out of range [-9, 0] dBTP");
        }
        Ok(vec![format!(
            "loudnorm=I={}:TP={}:LRA=11",
            self.lufs, self.peak
        )])
    }

    pub fn summary(&self) -> String {
        format!("{} LUFS · {} dBTP", self.lufs, self.peak)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BandConfig {
    /// Hz; 0 disables the high-pass.
    pub highpass: u32,
    /// Hz; 0 disables the low-pass.
    pub lowpass: u32,
}

impl Default for BandConfig {
    fn default() -> Self {
        Self {
            highpass: 80,
            lowpass: 0,
        }
    }
}

impl BandConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        let mut out = Vec::new();
        if self.highpass > 0 {
            out.push(format!("highpass=f={}", self.highpass));
        }
        if self.lowpass > 0 {
            out.push(format!("lowpass=f={}", self.lowpass));
        }
        Ok(out)
    }

    pub fn summary(&self) -> String {
        match (self.highpass, self.lowpass) {
            (0, 0) => tr!("off").into(),
            (hp, 0) => format!("hp {} Hz", hp),
            (0, lp) => format!("lp {} Hz", lp),
            (hp, lp) => format!("hp {} · lp {} Hz", hp, lp),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TempoConfig {
    /// 0.5 = half speed, 2 = double.
    pub factor: f32,
}

impl Default for TempoConfig {
    fn default() -> Self {
        Self { factor: 1.0 }
    }
}

impl TempoConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(0.5..=2.0).contains(&self.factor) {
            anyhow::bail!("tempo factor out of range [0.5, 2]");
        }
        if (self.factor - 1.0).abs() < 1e-3 {
            return Ok(Vec::new());
        }
        Ok(vec![format!("atempo={}", self.factor)])
    }

    pub fn summary(&self) -> String {
        if (self.factor - 1.0).abs() < 1e-3 {
            "1.00× (no change)".into()
        } else {
            format!("{:.2}×", self.factor)
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelsConfig {
    pub mode: ChannelMode,
}

impl ChannelsConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![format!(
            "aformat=channel_layouts={}",
            self.mode.layout()
        )])
    }

    pub fn summary(&self) -> String {
        self.mode.label().to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResampleConfig {
    pub rate: u32,
}

impl Default for ResampleConfig {
    fn default() -> Self {
        Self { rate: 48000 }
    }
}

impl ResampleConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        if !(8000..=192_000).contains(&self.rate) {
            anyhow::bail!("sample rate out of range [8000, 192000]");
        }
        Ok(vec![format!("aresample={}", self.rate)])
    }

    pub fn summary(&self) -> String {
        format!("{} Hz", self.rate)
    }
}

/// Escape hatch: one verbatim filtergraph fragment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RawFilterConfig {
    pub expr: String,
}

impl RawFilterConfig {
    pub fn filters(&self) -> anyhow::Result<Vec<String>> {
        let expr = self.expr.trim();
        if expr.is_empty() {
            return Ok(Vec::new());
        }
        if expr.contains(';') {
            anyhow::bail!("a raw filter cannot contain `;` (that would start a new graph segment)");
        }
        Ok(vec![expr.to_string()])
    }

    pub fn summary(&self) -> String {
        if self.expr.trim().is_empty() {
            tr!("empty").into()
        } else {
            short(self.expr.trim(), 30)
        }
    }
}

// ---------------------------------------------------------------------------
// Inspector forms
// ---------------------------------------------------------------------------

pub fn scale_form(ui: &mut egui::Ui, c: &mut ScaleConfig, changed: &mut bool) {
    segments(
        ui,
        tr!("Mode"),
        &mut c.mode,
        &ScaleMode::ALL.map(|m| (m, m.label())),
        changed,
    );
    text_row(ui, tr!("Width"), &mut c.width, changed);
    text_row(ui, tr!("Height"), &mut c.height, changed);
    // Presets sit flush left under the fields: with the label gutter they
    // would not fit on one line in a 360 px panel.
    wrap_row(ui, |ui| {
        ui.spacing_mut().button_padding.x = 6.0;
        ui.spacing_mut().item_spacing.x = 4.0;
        for (w, h, name) in [
            ("1920", "1080", "1080p"),
            ("1280", "720", "720p"),
            ("854", "480", "480p"),
            ("1080", "1920", "9:16"),
        ] {
            if ui.small_button(name).on_hover_text(format!("{}×{}", w, h)).clicked() {
                c.width = w.to_string();
                c.height = h.to_string();
                *changed = true;
            }
        }
    });
    ui.label(egui::RichText::new(c.mode.hint()).small().weak());
}

pub fn pad_form(ui: &mut egui::Ui, c: &mut PadConfig, changed: &mut bool) {
    text_row(ui, tr!("Width"), &mut c.width, changed);
    text_row(ui, tr!("Height"), &mut c.height, changed);
    text_row(ui, tr!("X"), &mut c.x, changed);
    text_row(ui, tr!("Y"), &mut c.y, changed);
    text_row(ui, tr!("Colour"), &mut c.color, changed);
    ui.label(
        egui::RichText::new(tr!("accepts expressions, e.g. (ow-iw)/2"))
            .small()
            .weak(),
    );
}

pub fn crop_form(ui: &mut egui::Ui, c: &mut CropConfig, changed: &mut bool) {
    text_row(ui, tr!("Width"), &mut c.width, changed);
    text_row(ui, tr!("Height"), &mut c.height, changed);
    text_row(ui, tr!("X"), &mut c.x, changed);
    text_row(ui, tr!("Y"), &mut c.y, changed);
    ui.label(
        egui::RichText::new(tr!("accepts iw / ih expressions"))
            .small()
            .weak(),
    );
}

pub fn rotate_form(ui: &mut egui::Ui, c: &mut RotateConfig, changed: &mut bool) {
    segments(
        ui,
        tr!("Rotate"),
        &mut c.rotate,
        &Rotate::ALL.map(|r| (r, r.label())),
        changed,
    );
    segments(
        ui,
        tr!("Flip"),
        &mut c.flip,
        &Flip::ALL.map(|f| (f, f.label())),
        changed,
    );
}

pub fn fps_form(ui: &mut egui::Ui, c: &mut FpsConfig, changed: &mut bool) {
    text_row(ui, tr!("FPS"), &mut c.fps, changed);
    ui.label(
        egui::RichText::new(tr!("fractional rates work too, e.g. 23.976"))
            .small()
            .weak(),
    );
}

pub fn color_form(ui: &mut egui::Ui, c: &mut ColorConfig, changed: &mut bool) {
    number_row_f32(ui, tr!("Brightness"), &mut c.brightness, -1.0..=1.0, 0.01, changed);
    number_row_f32(ui, tr!("Contrast"), &mut c.contrast, 0.0..=4.0, 0.01, changed);
    number_row_f32(ui, tr!("Saturation"), &mut c.saturation, 0.0..=3.0, 0.01, changed);
    number_row_f32(ui, tr!("Gamma"), &mut c.gamma, 0.1..=10.0, 0.01, changed);
    ui.label(
        egui::RichText::new(tr!("all-neutral values leave the picture untouched"))
            .small()
            .weak(),
    );
}

pub fn lut_form(ui: &mut egui::Ui, c: &mut LutConfig, changed: &mut bool) {
    path_row(
        ui,
        tr!("File"),
        &mut c.path,
        ("LUT", &["cube", "3dl", "dat", "m3d"]),
        changed,
    );
    segments(
        ui,
        tr!("Interp"),
        &mut c.interp,
        &LutInterp::ALL.map(|i| (i, i.label())),
        changed,
    );
    ui.label(
        egui::RichText::new(tr!(".cube / .3dl — usually placed before Sharpen"))
            .small()
            .weak(),
    );
}

pub fn sharpen_form(ui: &mut egui::Ui, c: &mut SharpenConfig, changed: &mut bool) {
    number_row_f32(ui, tr!("Amount"), &mut c.amount, 0.0..=3.0, 0.02, changed);
    ui.label(
        egui::RichText::new(tr!("0 disables the filter"))
            .small()
            .weak(),
    );
}

pub fn denoise_form(ui: &mut egui::Ui, c: &mut DenoiseConfig, changed: &mut bool) {
    number_row_f32(ui, tr!("Strength"), &mut c.strength, 0.0..=20.0, 0.1, changed);
    ui.label(
        egui::RichText::new(tr!("higher = smoother, more detail loss"))
            .small()
            .weak(),
    );
}

pub fn overlay_form(ui: &mut egui::Ui, c: &mut OverlayConfig, changed: &mut bool) {
    path_row(
        ui,
        tr!("Image"),
        &mut c.path,
        (tr!("Image"), &["png", "jpg", "jpeg", "webp", "bmp"]),
        changed,
    );
    text_row(ui, tr!("X"), &mut c.x, changed);
    text_row(ui, tr!("Y"), &mut c.y, changed);
    number_row_f32(ui, tr!("Scale"), &mut c.scale, 0.05..=4.0, 0.01, changed);
    ui.label(
        egui::RichText::new(tr!("the image is added as an extra ffmpeg input"))
            .small()
            .weak(),
    );
}

pub fn volume_form(ui: &mut egui::Ui, c: &mut VolumeConfig, changed: &mut bool) {
    number_row_f32(ui, tr!("Gain"), &mut c.db, -60.0..=24.0, 0.1, changed);
    ui.label(
        egui::RichText::new(tr!("dB; negative quietens, 0 leaves the level alone"))
            .small()
            .weak(),
    );
}

pub fn loudness_form(ui: &mut egui::Ui, c: &mut LoudnessConfig, changed: &mut bool) {
    number_row_f32(ui, tr!("Target"), &mut c.lufs, -70.0..=-5.0, 0.5, changed);
    number_row_f32(ui, tr!("True peak"), &mut c.peak, -9.0..=0.0, 0.1, changed);
    ui.label(
        egui::RichText::new(tr!("integrated LUFS; -14 … -16 for web"))
            .small()
            .weak(),
    );
}

pub fn band_form(ui: &mut egui::Ui, c: &mut BandConfig, changed: &mut bool) {
    number_row_u32(ui, tr!("High-pass"), &mut c.highpass, 0..=20000, changed);
    number_row_u32(ui, tr!("Low-pass"), &mut c.lowpass, 0..=20000, changed);
    ui.label(
        egui::RichText::new(tr!("Hz; 0 turns that side off"))
            .small()
            .weak(),
    );
}

pub fn tempo_form(ui: &mut egui::Ui, c: &mut TempoConfig, changed: &mut bool) {
    number_row_f32(ui, tr!("Factor"), &mut c.factor, 0.5..=2.0, 0.01, changed);
    ui.label(
        egui::RichText::new(tr!("0.5 = half speed, 2 = double"))
            .small()
            .weak(),
    );
}

pub fn channels_form(ui: &mut egui::Ui, c: &mut ChannelsConfig, changed: &mut bool) {
    segments(
        ui,
        tr!("Layout"),
        &mut c.mode,
        &ChannelMode::ALL.map(|m| (m, m.label())),
        changed,
    );
}

pub fn resample_form(ui: &mut egui::Ui, c: &mut ResampleConfig, changed: &mut bool) {
    number_row_u32(ui, tr!("Sample rate"), &mut c.rate, 8000..=192_000, changed);
}

pub fn raw_form(ui: &mut egui::Ui, c: &mut RawFilterConfig, changed: &mut bool) {
    text_row(ui, tr!("Filter"), &mut c.expr, changed);
    ui.label(
        egui::RichText::new(tr!("verbatim, e.g. setpts=0.5*PTS or aecho=0.8:0.9:1000"))
            .small()
            .weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_modes_emit_expected_filters() {
        let mut c = ScaleConfig {
            width: "1920".into(),
            height: "1080".into(),
            mode: ScaleMode::Exact,
        };
        assert_eq!(c.filters().unwrap(), vec!["scale=1920:1080"]);
        c.mode = ScaleMode::Fit;
        assert_eq!(
            c.filters().unwrap(),
            vec!["scale=1920:1080:force_original_aspect_ratio=decrease"]
        );
        c.mode = ScaleMode::Fill;
        assert_eq!(
            c.filters().unwrap(),
            vec![
                "scale=1920:1080:force_original_aspect_ratio=increase",
                "crop=1920:1080"
            ]
        );
        // `-2` keeps the ratio on the other axis
        c.mode = ScaleMode::Exact;
        c.height = "-2".into();
        assert_eq!(c.filters().unwrap(), vec!["scale=1920:-2"]);
        // both auto is meaningless
        c.width = "-2".into();
        assert!(c.filters().is_err());
    }

    #[test]
    fn pad_and_crop_build_their_expressions() {
        let pad = PadConfig::default();
        assert_eq!(
            pad.filters().unwrap(),
            vec!["pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=#000000"]
        );
        let crop = CropConfig::default();
        assert_eq!(
            crop.filters().unwrap(),
            vec!["crop=iw:ih:(iw-ow)/2:(ih-oh)/2"]
        );
        let mut bad = PadConfig::default();
        bad.height.clear();
        assert!(bad.filters().is_err());
    }

    #[test]
    fn lut_path_is_double_escaped() {
        let c = LutConfig {
            path: r"C:\luts\look.cube".into(),
            interp: LutInterp::Tetrahedral,
        };
        assert_eq!(
            c.filters().unwrap(),
            vec![r"lut3d=file=C\\:/luts/look.cube:interp=tetrahedral"]
        );
    }

    #[test]
    fn neutral_look_filters_are_passthrough() {
        assert!(ColorConfig::default().filters().unwrap().is_empty());
        assert!(SharpenConfig { amount: 0.0 }.filters().unwrap().is_empty());
        assert!(VolumeConfig { db: 0.0 }.filters().unwrap().is_empty());
        assert!(TempoConfig { factor: 1.0 }.filters().unwrap().is_empty());
        assert_eq!(ColorConfig::default().summary(), "neutral");
        assert_eq!(VolumeConfig::default().summary(), "0.0 dB (no change)");
    }

    #[test]
    fn audio_cards_emit_their_filters() {
        assert_eq!(
            LoudnessConfig::default().filters().unwrap(),
            vec!["loudnorm=I=-16:TP=-1.5:LRA=11"]
        );
        assert_eq!(
            BandConfig { highpass: 80, lowpass: 0 }.filters().unwrap(),
            vec!["highpass=f=80"]
        );
        assert_eq!(
            ChannelsConfig { mode: ChannelMode::Mono }.filters().unwrap(),
            vec!["aformat=channel_layouts=mono"]
        );
        assert_eq!(
            ResampleConfig { rate: 48000 }.filters().unwrap(),
            vec!["aresample=48000"]
        );
    }

    #[test]
    fn raw_filter_rejects_a_second_segment() {
        assert!(RawFilterConfig { expr: "setpts=0.5*PTS".into() }
            .filters()
            .unwrap()
            .len()
            == 1);
        assert!(RawFilterConfig { expr: "null;null".into() }.filters().is_err());
        assert!(RawFilterConfig::default().filters().unwrap().is_empty());
    }

    #[test]
    fn rotate_and_flip_summaries() {
        let c = RotateConfig {
            rotate: Rotate::Cw90,
            flip: Flip::Horizontal,
        };
        assert_eq!(c.filters().unwrap(), vec!["transpose=1", "hflip"]);
        assert_eq!(c.summary(), "90° · flip h");
        assert_eq!(RotateConfig::default().summary(), "passthrough");
    }
}
