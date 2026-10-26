//! Bridges fflow's node semantics to `egui_node_graph2`.
//!
//! The library only knows about generic ports; everything fflow-specific lives
//! here: the port data types ([`StreamType`]), the per-node payload
//! ([`NodeData`]), the node templates, and conversion to/from the saved
//! [`Workflow`] document.

pub mod legacy;
pub mod model;
pub mod nodes;

use std::borrow::Cow;
use std::collections::HashMap;

use egui_node_graph2::{
    DataTypeTrait, Graph, GraphEditorState, InputId, InputParamKind, NodeDataTrait, NodeId,
    NodeResponse, NodeTemplateIter, NodeTemplateTrait, OutputId, UserResponseTrait,
    WidgetValueTrait,
};

use crate::probe::ProbeInfo;

pub use model::{Edge, Settings, StreamType, WfNode, Workflow, WORKFLOW_VERSION};
pub use nodes::{NodeConfig, NodeKind};

// ---------------------------------------------------------------------------
// Ports / values
// ---------------------------------------------------------------------------

impl DataTypeTrait<GraphCtx> for StreamType {
    fn data_type_color(&self, _user_state: &mut GraphCtx) -> egui::Color32 {
        crate::theme::port_color(*self)
    }

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.name())
    }
}

/// Inputs are connection-only in fflow; this type exists to satisfy the
/// library's inline-widget machinery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InlineValue;

/// Node-level events the UI can react to.
#[derive(Debug, Clone)]
pub enum FlowResponse {
    /// Ask the app to ffprobe the first file of this source node.
    ProbeSource(u64),
}

impl UserResponseTrait for FlowResponse {}

/// Extra state shared while drawing the graph.
#[derive(Default)]
pub struct GraphCtx {
    /// Probe result of the file currently shown in the preview panel. Used to
    /// light up Demux ports.
    pub probe: Option<ProbeInfo>,
    /// Number of files each Source node resolved (keyed by JSON id).
    pub source_counts: HashMap<u64, usize>,
    /// Monotonic id generator for nodes in the saved document.
    pub next_id: u64,
}

// ---------------------------------------------------------------------------
// Node payload
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NodeData {
    /// Stable id inside the JSON document.
    pub json_id: u64,
    pub config: NodeConfig,
}

impl NodeData {
    pub fn kind(&self) -> NodeKind {
        self.config.kind()
    }
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeTemplate {
    pub kind: NodeKind,
}

pub struct AllTemplates;

impl NodeTemplateIter for AllTemplates {
    type Item = NodeTemplate;

    fn all_kinds(&self) -> Vec<Self::Item> {
        NodeKind::ALL
            .into_iter()
            .map(|kind| NodeTemplate { kind })
            .collect()
    }
}

pub fn build_ports(
    graph: &mut Graph<NodeData, StreamType, InlineValue>,
    node_id: NodeId,
    kind: NodeKind,
) {
    let (inputs, outputs) = kind.ports();
    // Merge points (encoders, Package) accept several connections; the
    // compiler resolves which branch is live for each file.
    let fan_in = kind.fan_in();
    for (name, typ) in inputs {
        // NOTE: `shown_inline` must stay true. egui_node_graph2 only measures
        // and draws input ports for params with `shown_inline == true`; with
        // false the port hook is never painted and any existing connection
        // panics while being drawn. `InlineValue::value_widget` just renders
        // the port label.
        graph.add_wide_input_param(
            node_id,
            name.to_string(),
            typ,
            InlineValue,
            InputParamKind::ConnectionOnly,
            if fan_in { None } else { std::num::NonZeroU32::new(1) },
            true,
        );
    }
    for (name, typ) in outputs {
        graph.add_output_param(node_id, name.to_string(), typ);
    }
}

impl NodeTemplateTrait for NodeTemplate {
    type NodeData = NodeData;
    type DataType = StreamType;
    type ValueType = InlineValue;
    type UserState = GraphCtx;
    type CategoryType = &'static str;

    fn node_finder_label(&self, _user_state: &mut GraphCtx) -> Cow<'_, str> {
        Cow::Borrowed(self.kind.title())
    }

    fn node_finder_categories(&self, _user_state: &mut GraphCtx) -> Vec<&'static str> {
        vec![self.kind.category()]
    }

    fn node_graph_label(&self, _user_state: &mut GraphCtx) -> String {
        self.kind.title().to_string()
    }

    fn user_data(&self, user_state: &mut GraphCtx) -> NodeData {
        let json_id = user_state.next_id;
        user_state.next_id += 1;
        NodeData {
            json_id,
            config: self.kind.default_config(),
        }
    }

    fn build_node(
        &self,
        graph: &mut Graph<NodeData, StreamType, InlineValue>,
        _user_state: &mut GraphCtx,
        node_id: NodeId,
    ) {
        build_ports(graph, node_id, self.kind);
    }
}

/// The zoom the node card is being laid out at.
///
/// `egui_node_graph2` zooms by swapping in a `Style::scaled(zoom)`, and it does
/// so *inside* `draw_graph_editor`. Anything we cache before that call is one
/// frame behind, which is exactly how a zoomed card ends up with a stale
/// width — so read the scale back out of the style that is actually in use.
fn style_zoom(ui: &egui::Ui) -> f32 {
    crate::theme::ui_zoom(ui)
}

/// Draw a port label against the right edge of the node body.
///
/// Output ports sit on the right, so their labels belong there too — the
/// default `output_ui` puts every label on the left and the card reads
/// backwards. Allocating the full body width also gives every card the same
/// width, which makes a chain look tidy.
///
/// Every constant here is multiplied by the zoom: this function runs in screen
/// pixels while the text around it is already scaled, so a bare `124.0` floor
/// would mean 124 px at *every* zoom — cards grew to 247 graph px at 0.68× and
/// shrank to 127 at 1.46×, which is what made zooming look broken.
///
/// The floor is deliberately narrow: it is the *minimum* card width, and a long
/// title still widens its own card. 146 px made every card 176 px wide, which
/// left a five-card pipeline wider than most windows.
fn right_label(ui: &mut egui::Ui, text: &str, dot: Option<egui::Color32>) {
    let zoom = style_zoom(ui);
    // Not `interact_size.y` (22 px): a port row is a dot and a word, and a
    // 22 px row was most of a card's height.
    let height = 16.0 * zoom;
    // Align to whatever the card has grown to so far, with a floor so a short
    // title does not turn the card into a narrow ribbon. Using the *available*
    // width instead would stretch every card to the library's 200 px maximum
    // and make neighbouring cards overlap.
    let left = ui.cursor().left();
    let right = ui.min_rect().right().max(left + 124.0 * zoom);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2((right - left).max(40.0 * zoom), height),
        egui::Sense::hover(),
    );
    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().text_color();
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, color);
    let pos = egui::pos2(
        rect.right() - galley.size().x,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(pos, galley, color);
    if let Some(c) = dot {
        ui.painter().circle_filled(
            egui::pos2(pos.x - 8.0 * zoom, rect.center().y),
            3.0 * zoom,
            c,
        );
    }
}

impl NodeDataTrait for NodeData {
    type Response = FlowResponse;
    type UserState = GraphCtx;
    type DataType = StreamType;
    type ValueType = InlineValue;

    fn bottom_ui(
        &self,
        ui: &mut egui::Ui,
        node_id: NodeId,
        graph: &Graph<Self, Self::DataType, Self::ValueType>,
        user_state: &mut Self::UserState,
    ) -> Vec<NodeResponse<Self::Response, Self>> {
        let mut responses = Vec::new();
        // Scaled: an unscaled 2 px is 8 graph px at 0.25x, which is enough to
        // make a zoomed-out card a little taller than a zoomed-in one.
        ui.add_space(1.0 * style_zoom(ui));
        // Demux draws its own (probe-based) summary below; showing the generic
        // one as well just repeats "v:1 · a:1" next to "v×1 / a×1".
        if !matches!(self.config, NodeConfig::Demux(_)) {
            ui.label(
                egui::RichText::new(self.config.summary())
                    .weak()
                    .small(),
            );
        }

        match &self.config {
            NodeConfig::Source(_) => {
                let count = user_state
                    .source_counts
                    .get(&self.json_id)
                    .copied()
                    .unwrap_or(0);
                if count > 0 {
                    ui.label(
                        egui::RichText::new(crate::tr!("{} files", count))
                            .small()
                            .color(egui::Color32::from_rgb(140, 200, 255)),
                    );
                }
                if crate::theme::mini_button(ui, crate::tr!("probe")).clicked() {
                    responses.push(NodeResponse::User(FlowResponse::ProbeSource(self.json_id)));
                }
            }
            NodeConfig::Demux(cfg) => {
                // Only call out the track selection when it is not the default
                // "first track of each type".
                if !cfg.is_default_modes() {
                    ui.label(
                        egui::RichText::new(cfg.summary())
                            .small()
                            .color(crate::theme::p().warn),
                    );
                }
                if let Some(probe) = &user_state.probe {
                    let count = |n: usize, what: &str| {
                        if n == 0 {
                            format!("no {}", what)
                        } else {
                            format!("{}×{}", what, n)
                        }
                    };
                    let v = count(probe.video_streams.len().max(usize::from(probe.has_video)), "v");
                    let a = count(probe.audio_streams.len().max(usize::from(probe.has_audio)), "a");
                    let s = probe.subtitle_streams.len();
                    let text = if s > 0 {
                        format!("{} / {} / s×{}", v, a, s)
                    } else {
                        format!("{} / {}", v, a)
                    };
                    ui.label(egui::RichText::new(text).small());
                } else {
                    ui.label(egui::RichText::new(crate::tr!("probe pending")).small().weak());
                }
            }
            NodeConfig::Package(_) => {
                let connected = graph
                    .nodes
                    .get(node_id)
                    .map(|n| {
                        n.inputs
                            .iter()
                            .filter(|(_, id)| !graph.connections(*id).is_empty())
                            .count()
                    })
                    .unwrap_or(0);
                if connected == 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 120, 100),
                        egui::RichText::new(crate::tr!("connect video and/or audio")).small(),
                    );
                }
            }
            _ => {}
        }

        responses
    }

    fn output_ui(
        &self,
        ui: &mut egui::Ui,
        _node_id: NodeId,
        _graph: &Graph<Self, Self::DataType, Self::ValueType>,
        user_state: &mut Self::UserState,
        param_name: &str,
    ) -> Vec<NodeResponse<Self::Response, Self>> {
        // Demux ports are "lit" only when ffprobe found that stream.
        if let NodeConfig::Demux(_) = &self.config {
            let available = match user_state.probe.as_ref() {
                None => None,
                Some(p) => Some(if param_name == "video" {
                    p.has_video
                } else if param_name == "audio" {
                    p.has_audio
                } else {
                    true
                }),
            };
            // Dot colour: the port's own media colour when the stream exists
            // (matching the edge), red when ffprobe says it does not.
            let media = if param_name == "audio" {
                crate::theme::port_color(crate::graph::StreamType::Audio)
            } else {
                crate::theme::port_color(crate::graph::StreamType::Video)
            };
            let color = match available {
                Some(true) => media,
                Some(false) => crate::theme::p().err.gamma_multiply(0.8),
                None => crate::theme::p().faint,
            };
            right_label(ui, param_name, Some(color));
            return Vec::new();
        }
        right_label(ui, param_name, None);
        Vec::new()
    }

    fn titlebar_color(
        &self,
        _ui: &egui::Ui,
        _node_id: NodeId,
        _graph: &Graph<Self, Self::DataType, Self::ValueType>,
        _user_state: &mut Self::UserState,
    ) -> Option<egui::Color32> {
        // The card is deliberately neutral grey; the family colour survives in
        // the library/inspector icon and the port dots. A tint of 8–10% here is
        // just enough to separate a video card from an audio one when they sit
        // side by side, and it keeps the title text (which the library paints
        // in the theme's text colour) readable.
        let cat = crate::theme::node_color(self.kind());
        let pal = crate::theme::p();
        Some(if crate::theme::is_dark() {
            crate::theme::mix(pal.card_top, cat, 0.10)
        } else {
            crate::theme::mix(pal.card_top, cat, 0.06)
        })
    }
}

impl WidgetValueTrait for InlineValue {
    type Response = FlowResponse;
    type UserState = GraphCtx;
    type NodeData = NodeData;

    fn value_widget(
        &mut self,
        param_name: &str,
        _node_id: NodeId,
        ui: &mut egui::Ui,
        _user_state: &mut Self::UserState,
        _node_data: &Self::NodeData,
    ) -> Vec<Self::Response> {
        ui.label(param_name);
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Editor type aliases and helpers
// ---------------------------------------------------------------------------

pub type FGraph = Graph<NodeData, StreamType, InlineValue>;
pub type GraphEditor = GraphEditorState<NodeData, StreamType, InlineValue, NodeTemplate, GraphCtx>;

/// Add a node of `kind` at graph-space `pos`, wiring its ports.
pub fn add_node(editor: &mut GraphEditor, ctx: &mut GraphCtx, kind: NodeKind, pos: egui::Pos2) -> NodeId {
    let json_id = ctx.next_id;
    ctx.next_id += 1;
    let node_id = editor.graph.add_node(
        kind.title().to_string(),
        NodeData {
            json_id,
            config: kind.default_config(),
        },
        |graph, node_id| build_ports(graph, node_id, kind),
    );
    editor.node_positions.insert(node_id, pos);
    editor.node_order.push(node_id);
    editor.selected_nodes = vec![node_id];
    node_id
}

/// Would connecting `output` → `input` create a cycle?
pub fn would_create_cycle(graph: &FGraph, output: OutputId, input: InputId) -> bool {
    let src = graph.get_output(output).node;
    let dst = graph.get_input(input).node;
    if src == dst {
        return true;
    }
    // Walk downstream from `dst`; if we reach `src`, the new edge closes a loop.
    let mut stack = vec![dst];
    let mut seen = std::collections::HashSet::new();
    while let Some(n) = stack.pop() {
        if n == src {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        for (_name, out) in &graph.nodes[n].outputs {
            for (i, conns) in graph.connections.iter() {
                if conns.contains(out) {
                    stack.push(graph.get_input(i).node);
                }
            }
        }
    }
    false
}

/// Check every connection for type consistency (defence in depth: the library
/// already refuses mismatched drops, but loaded workflows may be edited).
///
/// Uses the `try_` accessors because the graph can legitimately be transiently
/// inconsistent — see [`prune_dangling_connections`].
pub fn invalid_connections(graph: &FGraph) -> Vec<(InputId, OutputId, &'static str)> {
    let mut bad = Vec::new();
    for (input, outputs) in graph.iter_connection_groups() {
        let Some(in_param) = graph.try_get_input(input) else {
            continue;
        };
        for output in outputs {
            let Some(out_param) = graph.try_get_output(output) else {
                continue;
            };
            if in_param.typ != out_param.typ {
                bad.push((input, output, "type mismatch"));
            }
        }
    }
    bad
}

/// Drop the connection entries left behind by a node deletion.
///
/// `Graph::remove_node` removes the node plus its input/output parameters, and
/// it filters the *values* of `connections` so no edge dangles. It does not
/// remove the now-pointless `InputId` **keys** from the `SecondaryMap`, so the
/// map keeps handing out ids whose input parameter no longer exists. The next
/// `graph.get_input(that_id)` then panics with "invalid SlotMap key used" —
/// which is exactly what used to happen one frame after deleting a node.
pub fn prune_dangling_connections(graph: &mut FGraph) {
    let dead: Vec<InputId> = graph
        .connections
        .iter()
        .filter(|(input, _)| graph.try_get_input(*input).is_none())
        .map(|(input, _)| input)
        .collect();
    for input in dead {
        graph.connections.remove(input);
    }
}

pub fn node_by_json_id(graph: &FGraph, json_id: u64) -> Option<NodeId> {
    graph
        .nodes
        .iter()
        .find(|(_, n)| n.user_data.json_id == json_id)
        .map(|(id, _)| id)
}

pub fn package_nodes(graph: &FGraph) -> Vec<NodeId> {
    graph
        .nodes
        .iter()
        .filter(|(_, n)| n.user_data.kind() == NodeKind::Package)
        .map(|(id, _)| id)
        .collect()
}

pub fn source_nodes(graph: &FGraph) -> Vec<NodeId> {
    graph
        .nodes
        .iter()
        .filter(|(_, n)| n.user_data.kind() == NodeKind::Source)
        .map(|(id, _)| id)
        .collect()
}

// ---------------------------------------------------------------------------
// Document <-> editor conversion
// ---------------------------------------------------------------------------

pub fn to_workflow(editor: &GraphEditor) -> Workflow {
    // Node positions are stored in *screen* pixels, and `egui_node_graph2`
    // rescales them around the canvas centre every time the zoom changes. Saved
    // verbatim, a graph laid out at 0.6x comes back at 1.0x with its cards 40%
    // of the spacing they need — i.e. stacked on top of each other. Dividing by
    // the zoom turns them back into zoom-independent graph units, which is what
    // the loader assumes.
    let zoom = editor.pan_zoom.zoom.max(0.05);
    let mut nodes = Vec::new();
    for node_id in &editor.node_order {
        if let Some(node) = editor.graph.nodes.get(*node_id) {
            let pos = editor.node_positions.get(*node_id).copied();
            nodes.push(WfNode {
                id: node.user_data.json_id,
                kind: node.user_data.kind().id().to_string(),
                config: node.user_data.config.to_value(),
                pos: pos.map(|p| [p.x / zoom, p.y / zoom]),
            });
        }
    }

    let mut edges = Vec::new();
    for (input, output) in editor.graph.iter_connections() {
        let in_node = editor.graph.get_input(input).node;
        let out_node = editor.graph.get_output(output).node;
        let in_name = editor.graph.nodes[in_node]
            .inputs
            .iter()
            .find(|(_, id)| *id == input)
            .map(|(n, _)| n.clone());
        let out_name = editor.graph.nodes[out_node]
            .outputs
            .iter()
            .find(|(_, id)| *id == output)
            .map(|(n, _)| n.clone());
        if let (Some(to_port), Some(from_port)) = (in_name, out_name) {
            edges.push(Edge {
                from: (editor.graph.nodes[out_node].user_data.json_id, from_port),
                to: (editor.graph.nodes[in_node].user_data.json_id, to_port),
            });
        }
    }

    Workflow {
        version: WORKFLOW_VERSION.to_string(),
        name: "workflow".into(),
        nodes,
        edges,
    }
}

pub fn from_workflow(wf: &Workflow, ctx: &mut GraphCtx) -> anyhow::Result<GraphEditor> {
    let mut editor = GraphEditor::new(1.0);
    let mut map: HashMap<u64, NodeId> = HashMap::new();
    ctx.next_id = 0;

    for (i, node) in wf.nodes.iter().enumerate() {
        let kind = NodeKind::from_id(&node.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown node kind: {}", node.kind))?;
        let config = NodeConfig::from_value(kind, node.config.clone())
            .map_err(|e| anyhow::anyhow!("bad config for node {}: {}", node.id, e))?;
        let node_id = editor.graph.add_node(kind.title().to_string(), NodeData {
            json_id: node.id,
            config,
        }, |graph, node_id| build_ports(graph, node_id, kind));
        let pos = match node.pos {
            Some([x, y]) => egui::pos2(x, y),
            None => egui::pos2(60.0 + (i % 3) as f32 * 240.0, 60.0 + (i / 3) as f32 * 220.0),
        };
        editor.node_positions.insert(node_id, pos);
        editor.node_order.push(node_id);
        map.insert(node.id, node_id);
        ctx.next_id = ctx.next_id.max(node.id + 1);
    }

    for edge in &wf.edges {
        let (Some(&from_node), Some(&to_node)) = (map.get(&edge.from.0), map.get(&edge.to.0)) else {
            continue;
        };
        let out = editor.graph.nodes[from_node]
            .outputs
            .iter()
            .find(|(n, _)| *n == edge.from.1)
            .map(|(_, id)| *id);
        let inp = editor.graph.nodes[to_node]
            .inputs
            .iter()
            .find(|(n, _)| *n == edge.to.1)
            .map(|(_, id)| *id);
        if let (Some(out), Some(inp)) = (out, inp) {
            if editor.graph.get_output(out).typ == editor.graph.get_input(inp).typ {
                editor.graph.add_connection(out, inp, 0);
            }
        }
    }

    declutter(&mut editor);
    Ok(editor)
}

/// Push apart cards that were saved on top of each other.
///
/// Builds before this one stored node positions in screen pixels without
/// dividing by the canvas zoom, so a session saved at 60% reloads with its
/// cards at 60% of the spacing they need — visibly stacked, with one title bar
/// over its neighbour's body. Only colliding cards move, and only as far as the
/// next free slot, so a layout that was fine is left exactly as it was.
pub fn declutter(editor: &mut GraphEditor) {
    // Card footprint at 100% zoom (the loader always starts there). Height is
    // the tallest card (Package: two fan-in hook rows), not the average — a
    // short footprint let cards overlap vertically and the one in front
    // painted its ✕ over the body behind it.
    const CARD: egui::Vec2 = egui::vec2(152.0, 140.0);
    const GAP: f32 = 12.0;

    let mut placed: Vec<egui::Rect> = Vec::new();
    for id in editor.node_order.clone() {
        let Some(pos) = editor.node_positions.get(id).copied() else {
            continue;
        };
        let mut rect = egui::Rect::from_min_size(pos, CARD);
        // A chain of collisions can need more than one nudge; the guard keeps a
        // pathological layout from looping.
        for _ in 0..16 {
            // Move along whichever axis needs the shorter trip, so a card that
            // is only slightly too low drops down instead of jumping a whole
            // card to the right.
            let mut best = 0.0f32;
            let mut along_x = true;
            for r in placed.iter().filter(|r| r.intersects(rect)) {
                let right = r.right() + GAP - rect.left();
                let down = r.bottom() + GAP - rect.top();
                if best == 0.0 || right.min(down) < best {
                    best = right.min(down);
                    along_x = right <= down;
                }
            }
            if best <= 0.0 {
                break;
            }
            rect = rect.translate(if along_x {
                egui::vec2(best, 0.0)
            } else {
                egui::vec2(0.0, best)
            });
        }
        editor.node_positions.insert(id, rect.min);
        placed.push(rect);
    }
}

/// The graph a fresh fflow window starts with: a full, working pipeline.
pub fn default_workflow(root: Option<&std::path::Path>) -> Workflow {
    use serde_json::json;

    let mut source = nodes::SourceConfig::default();
    if let Some(root) = root {
        source.paths.push(root.to_string_lossy().to_string());
    }

    let mut wf = Workflow::default();
    wf.name = "default pipeline".into();
    // Spaced for the compact cards (148 px wide, ~115 tall at 100% zoom): a
    // 28 px gutter keeps the ports clear without wasting a third of the canvas.
    // The app fits this to the window on first layout, so the absolute numbers
    // only decide the aspect ratio of the starting view.
    wf.nodes = vec![
        WfNode {
            id: 0,
            kind: NodeKind::Source.id().into(),
            config: serde_json::to_value(&source).unwrap(),
            pos: Some([20.0, 50.0]),
        },
        WfNode {
            id: 1,
            kind: NodeKind::Demux.id().into(),
            config: json!({}),
            pos: Some([196.0, 50.0]),
        },
        WfNode {
            id: 2,
            kind: NodeKind::VideoEncoder.id().into(),
            config: serde_json::to_value(nodes::VideoEncoderConfig::default()).unwrap(),
            pos: Some([372.0, 10.0]),
        },
        WfNode {
            id: 3,
            kind: NodeKind::AudioEncoder.id().into(),
            config: serde_json::to_value(nodes::AudioEncoderConfig::default()).unwrap(),
            pos: Some([372.0, 214.0]),
        },
        WfNode {
            id: 4,
            kind: NodeKind::Package.id().into(),
            config: serde_json::to_value(nodes::PackageConfig::default()).unwrap(),
            pos: Some([548.0, 66.0]),
        },
    ];
    wf.edges = vec![
        Edge {
            from: (0, "file".into()),
            to: (1, "in".into()),
        },
        Edge {
            from: (1, "video".into()),
            to: (2, "in".into()),
        },
        Edge {
            from: (2, "out".into()),
            to: (4, "video".into()),
        },
        Edge {
            from: (1, "audio".into()),
            to: (3, "in".into()),
        },
        Edge {
            from: (3, "out".into()),
            to: (4, "audio".into()),
        },
    ];
    wf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_editor() -> (GraphEditor, GraphCtx) {
        editor()
    }

    fn editor() -> (GraphEditor, GraphCtx) {
        let mut ctx = GraphCtx::default();
        let wf = default_workflow(None);
        let editor = from_workflow(&wf, &mut ctx).unwrap();
        (editor, ctx)
    }

    fn port_by_name(
        graph: &FGraph,
        node: NodeId,
        name: &str,
        output: bool,
    ) -> egui_node_graph2::AnyParameterId {
        if output {
            let (_, id) = graph.nodes[node]
                .outputs
                .iter()
                .find(|(n, _)| n == name)
                .unwrap();
            egui_node_graph2::AnyParameterId::Output(*id)
        } else {
            let (_, id) = graph.nodes[node]
                .inputs
                .iter()
                .find(|(n, _)| n == name)
                .unwrap();
            egui_node_graph2::AnyParameterId::Input(*id)
        }
    }

    #[test]
    fn default_workflow_is_valid() {
        let (editor, _ctx) = editor();
        assert!(invalid_connections(&editor.graph).is_empty());
        assert_eq!(package_nodes(&editor.graph).len(), 1);
        assert_eq!(source_nodes(&editor.graph).len(), 1);
        // every package input is connected
        let pkg = package_nodes(&editor.graph)[0];
        for (_, input) in &editor.graph.nodes[pkg].inputs {
            assert_eq!(editor.graph.connections(*input).len(), 1);
        }
    }

    #[test]
    fn type_mismatch_is_detected() {
        let (mut editor, _ctx) = editor();
        let src = source_nodes(&editor.graph)[0];
        let demux = editor
            .graph
            .iter_nodes()
            .find(|n| editor.graph.nodes[*n].user_data.kind() == NodeKind::Demux)
            .unwrap();
        // File output → Video input of the encoder: illegal.
        let enc = editor
            .graph
            .iter_nodes()
            .find(|n| editor.graph.nodes[*n].user_data.kind() == NodeKind::VideoEncoder)
            .unwrap();
        let egui_node_graph2::AnyParameterId::Output(file_out) =
            port_by_name(&editor.graph, src, "file", true)
        else {
            unreachable!()
        };
        let egui_node_graph2::AnyParameterId::Input(enc_in) =
            port_by_name(&editor.graph, enc, "in", false)
        else {
            unreachable!()
        };
        editor.graph.add_connection(file_out, enc_in, 0);
        let bad = invalid_connections(&editor.graph);
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].2, "type mismatch");
        let _ = demux;
    }

    #[test]
    fn cycle_is_detected() {
        let (mut editor, mut ctx) = editor();
        let filter1 = add_node(&mut editor, &mut ctx, NodeKind::Scale, egui::pos2(0.0, 0.0));
        // A second filter chained after the first one.
        let filter2 = add_node(&mut editor, &mut ctx, NodeKind::Scale, egui::pos2(0.0, 0.0));
        let egui_node_graph2::AnyParameterId::Output(f1_out) =
            port_by_name(&editor.graph, filter1, "out", true)
        else {
            unreachable!()
        };
        let egui_node_graph2::AnyParameterId::Input(f2_in) =
            port_by_name(&editor.graph, filter2, "in", false)
        else {
            unreachable!()
        };
        editor.graph.add_connection(f1_out, f2_in, 0);

        // filter1 → filter2 exists, so filter2.out → filter1.in closes a loop.
        let egui_node_graph2::AnyParameterId::Output(f2_out) =
            port_by_name(&editor.graph, filter2, "out", true)
        else {
            unreachable!()
        };
        let egui_node_graph2::AnyParameterId::Input(f1_in) =
            port_by_name(&editor.graph, filter1, "in", false)
        else {
            unreachable!()
        };
        assert!(would_create_cycle(&editor.graph, f2_out, f1_in));
        // A legitimate forward connection is not a cycle.
        assert!(!would_create_cycle(&editor.graph, f1_out, f2_in));
    }

    #[test]
    fn roundtrip_preserves_graph() {
        let (editor, mut ctx) = editor();
        let wf = to_workflow(&editor);
        let json = wf.to_json().unwrap();
        let back = Workflow::from_json(&json).unwrap();
        let editor2 = from_workflow(&back, &mut ctx).unwrap();
        assert_eq!(editor.graph.nodes.len(), editor2.graph.nodes.len());
        assert_eq!(
            editor.graph.iter_connections().count(),
            editor2.graph.iter_connections().count()
        );
        assert_eq!(ctx.next_id, 5);
    }

    /// Positions are stored in screen pixels but saved in graph units.
    ///
    /// A session saved at 60% used to come back at 100% with its cards at 60%
    /// of the spacing they need — i.e. stacked on top of each other.
    #[test]
    fn saved_positions_do_not_depend_on_the_zoom() {
        let (mut editor, mut ctx) = editor();
        let before: Vec<egui::Pos2> = editor
            .node_order
            .iter()
            .map(|n| editor.node_positions[*n])
            .collect();

        // What the editor does on zoom-out: every position scales about the
        // canvas centre (the pan is left alone).
        editor.pan_zoom.zoom = 0.6;
        for pos in editor.node_positions.values_mut() {
            *pos = (*pos - egui::vec2(400.0, 260.0)) * 0.6 + egui::vec2(400.0, 260.0);
        }

        let wf = to_workflow(&editor);
        let editor2 = from_workflow(&wf, &mut ctx).unwrap();
        let after: Vec<egui::Pos2> = editor2
            .node_order
            .iter()
            .map(|n| editor2.node_positions[*n])
            .collect();
        // Absolute offsets are re-centred on load; what must survive is the
        // *spacing*, which is what keeps cards from stacking.
        for (i, (a, b)) in before.iter().zip(&after).enumerate() {
            for (j, (c, d)) in before.iter().zip(&after).enumerate().skip(i + 1) {
                let gap_before = *c - *a;
                let gap_after = *d - *b;
                assert!(
                    (gap_before - gap_after).length() < 1.0,
                    "nodes {i}/{j}: gap {:?} became {:?}",
                    gap_before,
                    gap_after
                );
            }
        }
    }

    /// Cards saved on top of each other are pushed apart on load, and a layout
    /// that was already fine is left alone.
    #[test]
    fn overlapping_cards_are_pushed_apart_on_load() {
        let mut wf = default_workflow(None);
        for node in &mut wf.nodes {
            node.pos = Some([40.0, 40.0]);
        }
        let mut ctx = GraphCtx::default();
        let editor = from_workflow(&wf, &mut ctx).unwrap();

        let rects: Vec<egui::Rect> = editor
            .node_order
            .iter()
            .map(|n| {
                egui::Rect::from_min_size(editor.node_positions[*n], egui::vec2(148.0, 113.0))
            })
            .collect();
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "cards still overlap: {a:?} / {b:?}");
            }
        }

        // The untouched default layout keeps its exact positions.
        let (clean, mut ctx2) = default_editor();
        let wf2 = to_workflow(&clean);
        let reloaded = from_workflow(&wf2, &mut ctx2).unwrap();
        for (a, b) in clean.node_order.iter().zip(&reloaded.node_order) {
            assert_eq!(clean.node_positions[*a], reloaded.node_positions[*b]);
        }
    }
    #[test]
    fn any_port_type_matches_everything() {
        use StreamType::*;
        // The Condition node relies on this: it has to sit in a video chain and
        // in an audio chain, and the editor validates drops with `==`.
        assert!(Any == File && Any == Video && Any == Audio && Any == Any);
        assert!(File == Any && Video == Any && Audio == Any);
        // everything else keeps its strict behaviour
        assert!(File != Video && Video != Audio && File != Audio);
    }
}

#[cfg(test)]
mod draw_tests {
    use super::*;

    #[test]
    fn draws_default_graph_without_panic() {
        let mut ctx = GraphCtx::default();
        let mut editor = from_workflow(&default_workflow(None), &mut ctx).unwrap();
        let egui_ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect =
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)));
        for _ in 0..3 {
            let out = egui_ctx.run(input.clone(), |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    let _ = editor.draw_graph_editor(ui, AllTemplates, &mut ctx, vec![]);
                });
            });
            let _ = out;
        }
        // describe the graph for debugging
        for (id, node) in editor.graph.nodes.iter() {
            println!(
                "node {:?} {} inputs={:?} outputs={:?}",
                id,
                node.label,
                node.inputs
                    .iter()
                    .map(|(n, i)| format!("{}:{:?}", n, editor.graph.connections(*i).len()))
                    .collect::<Vec<_>>(),
                node.outputs.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()
            );
        }
    }
}
