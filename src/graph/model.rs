//! Persistent workflow model: the JSON document that fflow saves and loads.
//!
//! The in-memory editor graph (`egui_node_graph2::Graph`) is converted to/from
//! this flat structure by [`crate::graph`]. Keeping the on-disk format simple
//! (nodes + edges + opaque config blobs) makes it stable across releases.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the on-disk document format.
pub const WORKFLOW_VERSION: &str = "1.0";

/// Port data type. Connections are only allowed between compatible types.
#[derive(Debug, Clone, Copy, Hash, Serialize, Deserialize)]
pub enum StreamType {
    File,
    Video,
    Audio,
    /// Matches any other type. Used by the Condition node, which has to sit
    /// inside a video *or* an audio chain.
    Any,
}

/// `Any` is compatible with everything, which is what makes the type check in
/// the graph editor accept a Condition node on either branch.
impl PartialEq for StreamType {
    fn eq(&self, other: &Self) -> bool {
        use StreamType::*;
        match (self, other) {
            (Any, _) | (_, Any) => true,
            (File, File) | (Video, Video) | (Audio, Audio) => true,
            _ => false,
        }
    }
}

impl Eq for StreamType {}

impl StreamType {
    pub fn name(&self) -> &'static str {
        match self {
            StreamType::File => "File",
            StreamType::Video => "Video",
            StreamType::Audio => "Audio",
            StreamType::Any => "Stream",
        }
    }
}

/// A node as stored on disk. `kind` is one of the strings returned by
/// `NodeKind::id()`; `config` is the serialized form of that node's config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WfNode {
    pub id: u64,
    pub kind: String,
    #[serde(default)]
    pub config: Value,
    /// Canvas position, purely cosmetic but needed for a faithful restore.
    #[serde(default)]
    pub pos: Option<[f32; 2]>,
}

/// `from` = (node id, output port name), `to` = (node id, input port name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: (u64, String),
    pub to: (u64, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<WfNode>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

impl Default for Workflow {
    fn default() -> Self {
        Self {
            version: WORKFLOW_VERSION.to_string(),
            name: "untitled".into(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl Workflow {
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let mut wf: Workflow = serde_json::from_str(text)?;
        // Files saved before the filter cards were split carry `video_filter`
        // / `audio_filter` nodes; expand them into the equivalent chain.
        crate::graph::legacy::migrate(&mut wf);
        Ok(wf)
    }
}

/// Persisted sizes of the five resizable regions of the window.
///
/// These are driven by our own splitters rather than by egui's panel handles —
/// see `FflowApp::splitters` for why.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PanelSizes {
    pub library: f32,
    pub inspector: f32,
    pub tasks: f32,
    pub preview: f32,
    pub files: f32,
}

impl Default for PanelSizes {
    fn default() -> Self {
        Self {
            library: 238.0,
            inspector: 360.0,
            tasks: 196.0,
            preview: 150.0,
            files: 128.0,
        }
    }
}

impl PanelSizes {
    pub const LIBRARY: std::ops::RangeInclusive<f32> = 180.0..=420.0;
    pub const INSPECTOR: std::ops::RangeInclusive<f32> = 280.0..=600.0;
    pub const TASKS: std::ops::RangeInclusive<f32> = 96.0..=520.0;
    /// The command preview needs room for its header plus three wrapped lines
    /// at 1080 px wide; below ~96 it clips a line in half at the panel edge.
    pub const PREVIEW: std::ops::RangeInclusive<f32> = 96.0..=400.0;
    pub const FILES: std::ops::RangeInclusive<f32> = 70.0..=520.0;

    /// Smallest canvas we are willing to leave behind, in both axes.
    pub const MIN_CANVAS: f32 = 320.0;
}

/// Application settings, persisted next to the config file (M8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub concurrency: usize,
    pub last_dir: String,
    pub last_workflow: String,
    pub dark_mode: bool,
    #[serde(default)]
    pub panels: PanelSizes,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            concurrency: 2,
            last_dir: String::new(),
            last_workflow: String::new(),
            dark_mode: true,
            panels: PanelSizes::default(),
        }
    }
}
