//! Migration for workflows saved before the filter cards were split up.
//!
//! A `video_filter` / `audio_filter` node used to carry every step as an
//! optional checkbox. That node no longer exists: on load it is expanded into
//! the chain of single-purpose cards it used to emit, in exactly the order the
//! old `chain()` produced, and the edges around it are re-wired. The result
//! compiles to the same filtergraph as before.
//!
//! The old `Condition` (two inputs `then`/`else`, one output `out`) is migrated
//! the same way: each connected branch becomes its own Condition with the same
//! rules, both feeding the same downstream port.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::graph::model::{Edge, WfNode, Workflow};
use crate::graph::nodes::{
    BandConfig, ChannelMode, ChannelsConfig, ColorConfig, CropConfig, DenoiseConfig, Flip,
    FpsConfig, LoudnessConfig, LutConfig, LutInterp, NodeKind, OverlayConfig, PadConfig,
    RawFilterConfig, ResampleConfig, Rotate, RotateConfig, ScaleConfig, ScaleMode, SharpenConfig,
    TempoConfig, VolumeConfig,
};

const CARD_GAP: f32 = 176.0;

// ---------------------------------------------------------------------------
// The old shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
enum OldScaleMode {
    #[default]
    Exact,
    Fit,
    Fill,
    Pad,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct OldVideo {
    scale: bool,
    width: String,
    height: String,
    scale_mode: OldScaleMode,
    pad_color: String,
    fps_enabled: bool,
    fps: String,
    crop_enabled: bool,
    crop_w: String,
    crop_h: String,
    crop_x: String,
    crop_y: String,
    rotate: Rotate,
    flip: Flip,
    eq_enabled: bool,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    gamma: f32,
    lut_enabled: bool,
    lut_path: String,
    lut_interp: LutInterp,
    sharpen_enabled: bool,
    sharpen: f32,
    denoise_enabled: bool,
    denoise: f32,
    overlay_enabled: bool,
    overlay_path: String,
    overlay_x: String,
    overlay_y: String,
    overlay_scale: f32,
    extra: String,
}

impl Default for OldVideo {
    fn default() -> Self {
        Self {
            scale: false,
            width: String::new(),
            height: String::new(),
            scale_mode: OldScaleMode::Exact,
            pad_color: "black".into(),
            fps_enabled: false,
            fps: "30".into(),
            crop_enabled: false,
            crop_w: "iw".into(),
            crop_h: "ih".into(),
            crop_x: "0".into(),
            crop_y: "0".into(),
            rotate: Rotate::None,
            flip: Flip::None,
            eq_enabled: false,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
            lut_enabled: false,
            lut_path: String::new(),
            lut_interp: LutInterp::Tetrahedral,
            sharpen_enabled: false,
            sharpen: 1.0,
            denoise_enabled: false,
            denoise: 3.0,
            overlay_enabled: false,
            overlay_path: String::new(),
            overlay_x: "0".into(),
            overlay_y: "0".into(),
            overlay_scale: 1.0,
            extra: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct OldAudio {
    highpass_enabled: bool,
    highpass: u32,
    lowpass_enabled: bool,
    lowpass: u32,
    volume_enabled: bool,
    volume: f32,
    atempo_enabled: bool,
    atempo: f32,
    loudnorm_enabled: bool,
    lufs: f32,
    channels_enabled: bool,
    channels: ChannelMode,
    resample_enabled: bool,
    sample_rate: u32,
    extra: String,
}

impl Default for OldAudio {
    fn default() -> Self {
        Self {
            highpass_enabled: false,
            highpass: 80,
            lowpass_enabled: false,
            lowpass: 12000,
            volume_enabled: false,
            volume: 1.0,
            atempo_enabled: false,
            atempo: 1.0,
            loudnorm_enabled: false,
            lufs: -16.0,
            channels_enabled: false,
            channels: ChannelMode::Stereo,
            resample_enabled: false,
            sample_rate: 48000,
            extra: String::new(),
        }
    }
}

fn value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// The chain a `video_filter` node used to compile to, as cards.
fn video_cards(old: &OldVideo) -> Vec<(NodeKind, Value)> {
    let mut out = Vec::new();

    if old.crop_enabled {
        out.push((
            NodeKind::Crop,
            value(&CropConfig {
                width: old.crop_w.clone(),
                height: old.crop_h.clone(),
                x: old.crop_x.clone(),
                y: old.crop_y.clone(),
            }),
        ));
    }

    if old.scale {
        // `pad` used to be a scale mode that emitted scale + pad; it is now a
        // card of its own, so the mode becomes `fit` and the pad follows.
        let (mode, pad) = match old.scale_mode {
            OldScaleMode::Exact => (ScaleMode::Exact, false),
            OldScaleMode::Fit => (ScaleMode::Fit, false),
            OldScaleMode::Fill => (ScaleMode::Fill, false),
            OldScaleMode::Pad => (ScaleMode::Fit, true),
        };
        out.push((
            NodeKind::Scale,
            value(&ScaleConfig {
                width: old.width.clone(),
                height: old.height.clone(),
                mode,
            }),
        ));
        if pad {
            out.push((
                NodeKind::Pad,
                value(&PadConfig {
                    width: old.width.clone(),
                    height: old.height.clone(),
                    ..Default::default()
                }),
            ));
            if let Some((_, Value::Object(map))) = out.last_mut() {
                map.insert("color".into(), Value::String(old.pad_color.clone()));
            }
        }
    }

    if old.fps_enabled {
        out.push((
            NodeKind::Fps,
            value(&FpsConfig {
                fps: old.fps.clone(),
            }),
        ));
    }

    if old.rotate != Rotate::None || old.flip != Flip::None {
        out.push((
            NodeKind::Rotate,
            value(&RotateConfig {
                rotate: old.rotate,
                flip: old.flip,
            }),
        ));
    }

    if old.eq_enabled {
        out.push((
            NodeKind::Color,
            value(&ColorConfig {
                brightness: old.brightness,
                contrast: old.contrast,
                saturation: old.saturation,
                gamma: old.gamma,
            }),
        ));
    }

    if old.lut_enabled {
        out.push((
            NodeKind::Lut,
            value(&LutConfig {
                path: old.lut_path.clone(),
                interp: old.lut_interp,
            }),
        ));
    }

    if old.sharpen_enabled {
        out.push((
            NodeKind::Sharpen,
            value(&SharpenConfig {
                amount: old.sharpen,
            }),
        ));
    }

    if old.denoise_enabled {
        out.push((
            NodeKind::Denoise,
            value(&DenoiseConfig {
                strength: old.denoise,
            }),
        ));
    }

    if old.overlay_enabled {
        out.push((
            NodeKind::Overlay,
            value(&OverlayConfig {
                path: old.overlay_path.clone(),
                x: old.overlay_x.clone(),
                y: old.overlay_y.clone(),
                scale: old.overlay_scale,
            }),
        ));
    }

    if !old.extra.trim().is_empty() {
        out.push((
            NodeKind::RawFilter,
            value(&RawFilterConfig {
                expr: old.extra.clone(),
            }),
        ));
    }

    out
}

/// The chain an `audio_filter` node used to compile to, as cards.
fn audio_cards(old: &OldAudio) -> Vec<(NodeKind, Value)> {
    let mut out = Vec::new();

    if old.highpass_enabled || old.lowpass_enabled {
        out.push((
            NodeKind::Band,
            value(&BandConfig {
                highpass: if old.highpass_enabled { old.highpass } else { 0 },
                lowpass: if old.lowpass_enabled { old.lowpass } else { 0 },
            }),
        ));
    }

    // The old gain was a linear factor; the Volume card speaks dB. Rounded to
    // two decimals, because `20*log10(1.5)` prints as 3.5218253 dB otherwise.
    if old.volume_enabled && (old.volume - 1.0).abs() > 1e-4 {
        let db = 20.0 * old.volume.max(1e-4).log10();
        let db = (db.clamp(-60.0, 24.0) * 100.0).round() / 100.0;
        out.push((NodeKind::Volume, value(&VolumeConfig { db })));
    }

    if old.atempo_enabled && (old.atempo - 1.0).abs() > 1e-4 {
        out.push((
            NodeKind::Tempo,
            value(&TempoConfig {
                factor: old.atempo,
            }),
        ));
    }

    if old.loudnorm_enabled {
        out.push((
            NodeKind::Loudness,
            value(&LoudnessConfig {
                lufs: old.lufs,
                peak: -1.5,
            }),
        ));
    }

    if old.channels_enabled {
        out.push((
            NodeKind::Channels,
            value(&ChannelsConfig {
                mode: old.channels,
            }),
        ));
    }

    if old.resample_enabled {
        out.push((
            NodeKind::Resample,
            value(&ResampleConfig {
                rate: old.sample_rate,
            }),
        ));
    }

    if !old.extra.trim().is_empty() {
        out.push((
            NodeKind::RawFilter,
            value(&RawFilterConfig {
                expr: old.extra.clone(),
            }),
        ));
    }

    out
}

// ---------------------------------------------------------------------------
// Graph rewrite
// ---------------------------------------------------------------------------

/// Rewrite every legacy node in `wf` in place. Returns the number of nodes that
/// were expanded, so the caller can tell the user.
pub fn migrate(wf: &mut Workflow) -> usize {
    let mut expanded = 0;

    // --- filter nodes: one node in, N nodes out ---------------------------
    let mut next_id = wf.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
    let mut replacements: HashMap<u64, Vec<WfNode>> = HashMap::new();

    for node in &wf.nodes {
        let cards: Vec<(NodeKind, Value)> = match node.kind.as_str() {
            "video_filter" => {
                let old: OldVideo = serde_json::from_value(node.config.clone()).unwrap_or_default();
                video_cards(&old)
            }
            "audio_filter" => {
                let old: OldAudio = serde_json::from_value(node.config.clone()).unwrap_or_default();
                audio_cards(&old)
            }
            _ => continue,
        };
        expanded += 1;

        let [x, y] = node.pos.unwrap_or([60.0, 60.0]);
        let made: Vec<WfNode> = cards
            .into_iter()
            .enumerate()
            .map(|(i, (kind, config))| WfNode {
                id: next_id + i as u64,
                kind: kind.id().into(),
                config,
                pos: Some([x + i as f32 * CARD_GAP, y]),
            })
            .collect();
        next_id += made.len() as u64;
        replacements.insert(node.id, made);
    }
    if expanded > 0 {
        // Lay the replacement chain out left to right and push everything that
        // used to sit to its right further right, so the chain never overlaps.
        let shifts: Vec<(f32, f32)> = replacements
            .iter()
            .filter_map(|(id, made)| {
                let old_x = wf
                    .nodes
                    .iter()
                    .find(|n| n.id == *id)
                    .and_then(|n| n.pos)
                    .map(|p| p[0])?;
                let extra = made.len().saturating_sub(1) as f32 * CARD_GAP;
                Some((old_x, extra))
            })
            .collect();

        let mut nodes = Vec::with_capacity(wf.nodes.len());
        for mut node in std::mem::take(&mut wf.nodes) {
            if let Some(made) = replacements.get(&node.id) {
                nodes.extend(made.iter().cloned());
                continue;
            }
            if let Some([x, y]) = node.pos {
                let dx: f32 = shifts
                    .iter()
                    .filter(|(old_x, _)| x > *old_x)
                    .map(|(_, d)| *d)
                    .sum();
                node.pos = Some([x + dx, y]);
            }
            nodes.push(node);
        }
        wf.nodes = nodes;

        // Re-wire edges around the replaced node.
        let mut edges: Vec<Edge> = Vec::with_capacity(wf.edges.len());
        for edge in std::mem::take(&mut wf.edges) {
            let made = replacements.get(&edge.from.0);
            let incoming = replacements.get(&edge.to.0);
            match (made, incoming) {
                (Some(made), _) if edge.from.1 == "out" => {
                    if let Some(last) = made.last() {
                        edges.push(Edge {
                            from: (last.id, "out".into()),
                            to: edge.to.clone(),
                        });
                    }
                }
                (_, Some(made)) if edge.to.1 == "in" => {
                    if let Some(first) = made.first() {
                        edges.push(Edge {
                            from: edge.from.clone(),
                            to: (first.id, "in".into()),
                        });
                    }
                }
                _ => edges.push(edge),
            }
        }

        // The cards of one chain have to be wired to each other, otherwise the
        // graph is a pile of disconnected nodes that compiles to nothing.
        for made in replacements.values() {
            for pair in made.windows(2) {
                edges.push(Edge {
                    from: (pair[0].id, "out".into()),
                    to: (pair[1].id, "in".into()),
                });
            }
        }

        wf.edges = edges;
    }

    // --- old Condition: two inputs, one output ----------------------------
    let conditions: Vec<u64> = wf
        .nodes
        .iter()
        .filter(|n| n.kind == "condition")
        .filter(|n| {
            wf.edges
                .iter()
                .any(|e| e.to.0 == n.id && (e.to.1 == "then" || e.to.1 == "else"))
        })
        .map(|n| n.id)
        .collect();

    for id in conditions {
        expanded += 1;

        let then_src = wf
            .edges
            .iter()
            .find(|e| e.to.0 == id && e.to.1 == "then")
            .map(|e| e.from.clone());
        let else_src = wf
            .edges
            .iter()
            .find(|e| e.to.0 == id && e.to.1 == "else")
            .map(|e| e.from.clone());
        let out_targets: Vec<(u64, String)> = wf
            .edges
            .iter()
            .filter(|e| e.from.0 == id && e.from.1 == "out")
            .map(|e| e.to.clone())
            .collect();

        // `pass` for the old `then` branch, `fail` for the old `else` branch.
        // Both sides connected means two Condition cards with the same rules.
        let mut branches: Vec<(u64, (u64, String), &str)> = Vec::new();
        if let Some(src) = then_src.clone() {
            branches.push((id, src, "pass"));
        }
        if let Some(src) = else_src.clone() {
            let copy_id = if branches.is_empty() {
                // Only `else` was wired: reuse the node and route through fail.
                id
            } else {
                next_id += 1;
                if let Some(node) = wf.nodes.iter().find(|n| n.id == id) {
                    let mut copy = node.clone();
                    copy.id = next_id - 1;
                    copy.pos = copy.pos.map(|[x, y]| [x, y + 130.0]);
                    wf.nodes.push(copy);
                }
                next_id - 1
            };
            branches.push((copy_id, src, "fail"));
        }
        if branches.is_empty() {
            continue;
        }

        // Drop the old edges, then rebuild.
        wf.edges.retain(|e| !(e.from.0 == id || e.to.0 == id));

        for (node_id, src, port) in branches {
            wf.edges.push(Edge {
                from: src,
                to: (node_id, "in".into()),
            });
            for target in &out_targets {
                wf.edges.push(Edge {
                    from: (node_id, port.into()),
                    to: target.clone(),
                });
            }
        }
    }

    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::nodes::NodeConfig;

    /// A workflow saved before the split: one `video_filter` and one
    /// `audio_filter` node carrying every option at once.
    const OLD: &str = r#"{
      "version": "1",
      "name": "old kitchen sink",
      "nodes": [
        {"id": 0, "kind": "source", "config": {}},
        {"id": 1, "kind": "demux", "config": {}},
        {"id": 2, "kind": "video_filter", "pos": [300, 10], "config": {
          "crop_enabled": true, "crop_w": "640", "crop_h": "360",
          "crop_x": "10", "crop_y": "20",
          "scale": true, "width": "1280", "height": "720",
          "scale_mode": "Pad", "pad_color": "white",
          "fps_enabled": true, "fps": "30",
          "rotate": "Cw90", "flip": "Horizontal",
          "eq_enabled": true, "brightness": 0.1, "contrast": 1.2,
          "saturation": 0.9, "gamma": 1.0,
          "lut_enabled": true, "lut_path": "C:/luts/a.cube",
          "lut_interp": "Tetrahedral",
          "sharpen_enabled": true, "sharpen": 1.5,
          "denoise_enabled": true, "denoise": 4.0,
          "overlay_enabled": true, "overlay_path": "C:/art/logo.png",
          "overlay_x": "5", "overlay_y": "5", "overlay_scale": 1.0,
          "extra": "setsar=1"
        }},
        {"id": 3, "kind": "audio_filter", "pos": [300, 250], "config": {
          "highpass_enabled": true, "highpass": 80,
          "lowpass_enabled": true, "lowpass": 12000,
          "volume_enabled": true, "volume": 1.5,
          "atempo_enabled": true, "atempo": 1.25,
          "loudnorm_enabled": true, "lufs": -16.0,
          "channels_enabled": true, "channels": "Stereo",
          "resample_enabled": true, "sample_rate": 48000
        }},
        {"id": 4, "kind": "video_encoder", "config": {}},
        {"id": 5, "kind": "audio_encoder", "config": {}},
        {"id": 6, "kind": "package", "config": {}}
      ],
      "edges": [
        {"from": [0, "file"], "to": [1, "in"]},
        {"from": [1, "video"], "to": [2, "in"]},
        {"from": [2, "out"], "to": [4, "in"]},
        {"from": [4, "out"], "to": [6, "video"]},
        {"from": [1, "audio"], "to": [3, "in"]},
        {"from": [3, "out"], "to": [5, "in"]},
        {"from": [5, "out"], "to": [6, "audio"]}
      ]
    }"#;

    fn load() -> Workflow {
        Workflow::from_json(OLD).expect("old workflow parses")
    }

    /// The expanded chain keeps the order the old `chain()` emitted.
    #[test]
    fn video_filter_expands_in_the_old_order() {
        let wf = load();
        let chain: Vec<&str> = wf
            .nodes
            .iter()
            .filter(|n| n.id >= 7 && n.id <= 16)
            .map(|n| n.kind.as_str())
            .collect();
        assert_eq!(
            chain,
            vec![
                "crop", "scale", "pad", "fps", "rotate", "color", "lut", "sharpen", "denoise",
                "overlay"
            ]
        );
        // `extra` becomes a Raw card after the overlay.
        assert!(wf.nodes.iter().any(|n| n.kind == "raw_filter"));
    }

    #[test]
    fn pad_scale_mode_splits_into_two_cards() {
        let wf = load();
        let scale = wf.nodes.iter().find(|n| n.kind == "scale").unwrap();
        let cfg: ScaleConfig = serde_json::from_value(scale.config.clone()).unwrap();
        assert_eq!(cfg.mode, ScaleMode::Fit);
        let pad = wf.nodes.iter().find(|n| n.kind == "pad").unwrap();
        let cfg: PadConfig = serde_json::from_value(pad.config.clone()).unwrap();
        assert_eq!((cfg.width.as_str(), cfg.height.as_str()), ("1280", "720"));
        assert_eq!(cfg.color, "white");
    }

    /// The rewired graph compiles to exactly the filtergraph the monolithic
    /// card used to emit — this is the contract the migration has to keep.
    #[test]
    fn migration_keeps_the_filtergraph() {
        let wf = load();
        let mut ctx = crate::graph::GraphCtx::default();
        let editor = crate::graph::from_workflow(&wf, &mut ctx).expect("graph builds");
        let probe = crate::probe::ProbeInfo {
            has_video: true,
            has_audio: true,
            duration: 1.0,
            ..Default::default()
        };
        let job = crate::compiler::compile(
            &editor.graph,
            std::path::Path::new("in.mp4"),
            &probe,
            1,
            None,
        )
        .expect("compiles");

        let fc = job
            .args
            .iter()
            .position(|a| a == "-filter_complex")
            .map(|i| job.args[i + 1].clone())
            .unwrap_or_default();

        let video = concat!(
            "[0:v]crop=640:360:10:20,",
            "scale=1280:720:force_original_aspect_ratio=decrease,",
            "pad=1280:720:(ow-iw)/2:(oh-ih)/2:color=white,",
            "fps=30,transpose=1,hflip,",
            "eq=brightness=0.1:contrast=1.2:saturation=0.9:gamma=1,",
            "lut3d=file=C\\\\:/luts/a.cube:interp=tetrahedral,",
            "unsharp=5:5:1.5:5:5:0,hqdn3d=4[t0];",
            "[t0][1:v]overlay=5:5:shortest=1[t1];[t1]setsar=1[v]"
        );
        let audio = concat!(
            "[0:a]highpass=f=80,lowpass=f=12000,volume=3.52dB,atempo=1.25,",
            "loudnorm=I=-16:TP=-1.5:LRA=11,",
            "aformat=channel_layouts=stereo,aresample=48000[a]"
        );
        assert_eq!(fc, format!("{};{}", video, audio));
    }

    /// An old Condition (two inputs `then`/`else`, one output) becomes one
    /// Condition per wired branch, with the same rules.
    #[test]
    fn old_condition_splits_into_two() {
        let mut wf = load();
        wf.nodes.push(WfNode {
            id: 90,
            kind: "condition".into(),
            config: serde_json::json!({"label": "is mp4"}),
            pos: Some([800.0, 10.0]),
        });
        // `then`/`else` used to be *inputs*; both feed the video encoder.
        wf.edges.push(Edge {
            from: (1, "video".into()),
            to: (90, "then".into()),
        });
        wf.edges.push(Edge {
            from: (1, "video".into()),
            to: (90, "else".into()),
        });
        wf.edges.push(Edge {
            from: (90, "out".into()),
            to: (4, "in".into()),
        });

        let expanded = migrate(&mut wf);
        // `load()` already expanded the filters, so only the condition is left.
        assert_eq!(expanded, 1);
        let conditions: Vec<&WfNode> =
            wf.nodes.iter().filter(|n| n.kind == "condition").collect();
        assert_eq!(conditions.len(), 2, "one Condition per branch");
        // The copy keeps the rules and sits below the original.
        let labels: Vec<String> = conditions
            .iter()
            .map(|n| {
                match NodeConfig::from_value(NodeKind::Condition, n.config.clone()) {
                    Ok(NodeConfig::Condition(c)) => c.label.clone(),
                    _ => panic!("not a condition"),
                }
            })
            .collect();
        assert_eq!(labels, vec!["is mp4", "is mp4"]);
        assert!(wf.edges.iter().any(|e| e.from.1 == "pass"));
        assert!(wf.edges.iter().any(|e| e.from.1 == "fail"));
        assert!(!wf.edges.iter().any(|e| e.to.1 == "then" || e.to.1 == "else"));
    }
}
