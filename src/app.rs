//! Application state, layout and the event pump.
//!
//! Layout: node library (left) · node graph (center) · inspector (right),
//! with a toolbar on top, a command preview and the task list at the bottom.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_node_graph2::NodeResponse;

use crate::compiler::{self, Job};
use crate::graph::nodes::{self, NodeConfig, NodeKind};
use crate::graph::model;
use crate::graph::{self, AllTemplates, FlowResponse, GraphCtx, GraphEditor, Settings, Workflow};
use crate::probe::{self, ProbeInfo};
use crate::runner::{RunEvent, Runner};
use crate::source;
use crate::theme;

/// Vertical room the node library needs before the file list may eat the rest
/// of the side panel.
const LIBRARY_MIN_NODES: f32 = 140.0;

/// Gap between the node list and the divider above the file list, so the list
/// is clipped in empty panel space instead of through a card.
const LIBRARY_GUTTER: f32 = 6.0;

/// Card footprint used for "fit the graph" maths before any card has been
/// measured. Only the first frame uses it, and `fit_view` pads around it.
const EST_CARD_W: f32 = 150.0;
const EST_CARD_H: f32 = 115.0;

/// Never fit the graph below this, or a big pipeline turns into unreadable
/// confetti. Below it the user pans instead.
const MIN_FIT_ZOOM: f32 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl TaskStatus {
    fn label(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Running => "running",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    fn color(&self) -> egui::Color32 {
        let pal = theme::p();
        match self {
            TaskStatus::Queued => pal.faint,
            TaskStatus::Running => pal.accent,
            TaskStatus::Done => pal.ok,
            TaskStatus::Failed => pal.err,
            TaskStatus::Cancelled => pal.warn,
        }
    }
}

pub struct TaskRow {
    pub id: usize,
    pub input: PathBuf,
    pub output: PathBuf,
    pub status: TaskStatus,
    pub pct: f32,
    pub speed: String,
    pub out_time: f64,
    pub duration: f64,
    pub error: Option<String>,
    pub secs: f64,
    pub expanded: bool,
}

struct Toast {
    msg: String,
    born: Instant,
    error: bool,
}

/// Where each panel actually ended up last frame.
///
/// The splitters used to re-derive these from `settings.panels` plus the screen
/// rect, which is off by the panel's own `inner_margin` (the file list is a
/// panel nested *inside* the library's padded content, so its top edge sits 8 px
/// above `preview_top - files`). Reading the rects back from the layout means a
/// handle is always on the edge that was drawn, whatever the padding is.
#[derive(Debug, Clone, Copy)]
struct PanelRects {
    library: egui::Rect,
    /// The library panel *inside* its frame margin. The file list is a panel
    /// nested in this rect, so this is what limits how tall it may grow.
    library_content: egui::Rect,
    /// The file list, nested inside the library panel.
    files: egui::Rect,
    inspector: egui::Rect,
    tasks: egui::Rect,
    preview: egui::Rect,
}

impl Default for PanelRects {
    fn default() -> Self {
        Self {
            library: egui::Rect::NOTHING,
            library_content: egui::Rect::NOTHING,
            files: egui::Rect::NOTHING,
            inspector: egui::Rect::NOTHING,
            tasks: egui::Rect::NOTHING,
            preview: egui::Rect::NOTHING,
        }
    }
}

pub struct FflowApp {
    editor: GraphEditor,
    ctx: GraphCtx,
    runner: Runner,

    settings: Settings,
    root: Option<PathBuf>,
    workflow_path: Option<PathBuf>,

    files: Vec<PathBuf>,
    probes: HashMap<String, ProbeInfo>,
    probe_inflight: HashSet<String>,
    /// Files ffprobe refused to read. Kept so a broken file is not re-probed on
    /// every frame — `refresh_files` clears it so "rescan" tries again.
    probe_failed: HashSet<String>,
    files_dirty_at: Option<Instant>,
    probe_tx: tokio::sync::mpsc::UnboundedSender<probe::ProbeResult>,
    probe_rx: tokio::sync::mpsc::UnboundedReceiver<probe::ProbeResult>,

    tasks: Vec<TaskRow>,
    library_filter: String,
    /// Library categories the user folded away; a search ignores this.
    collapsed_cats: Vec<String>,
    drag_kind: Option<NodeKind>,
    canvas_rect: egui::Rect,
    /// Outer rect of the toolbar, so the splitters know where the panels start.
    toolbar_rect: egui::Rect,
    /// Actual panel rects from the last layout, used to place the splitters.
    panel_rects: PanelRects,
    /// The canvas has been zoomed to fit the workflow once, at first layout.
    view_fitted: bool,
    preview_index: usize,
    show_preview: bool,
    show_help: bool,
    toasts: Vec<Toast>,
    tools_error: Option<String>,
}

impl FflowApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        // Kick off encoder detection once, in the background, before the first
        // frame. Deliberately not in `draw_all`: the headless tests drive that
        // and must not spawn subprocesses.
        crate::encoders::ensure_detection();
        Self::build(&cc.egui_ctx, true)
    }

    /// Shared constructor; `autoload` restores the last session (the headless
    /// UI test passes false to stay deterministic).
    pub fn build(egui_ctx: &egui::Context, autoload: bool) -> Self {
        let settings = load_settings();
        crate::theme::apply(egui_ctx, settings.dark_mode);

        let mut ctx = GraphCtx::default();
        let workflow = if autoload {
            load_autosave().unwrap_or_else(|| {
                graph::default_workflow(
                    std::env::current_dir()
                        .ok()
                        .as_deref()
                        .filter(|p| p.is_dir()),
                )
            })
        } else {
            graph::default_workflow(None)
        };
        let editor = match graph::from_workflow(&workflow, &mut ctx) {
            Ok(e) => e,
            Err(_) => {
                ctx = GraphCtx::default();
                graph::from_workflow(&graph::default_workflow(None), &mut ctx)
                    .expect("default workflow must load")
            }
        };

        let (probe_tx, probe_rx) = tokio::sync::mpsc::unbounded_channel();
        let runner = Runner::new().expect("tokio runtime");
        let tools_error = check_tools();

        let root = workflow_root(&editor);
        let mut app = Self {
            editor,
            ctx,
            runner,
            settings,
            root,
            workflow_path: None,
            files: Vec::new(),
            probes: HashMap::new(),
            probe_inflight: HashSet::new(),
            probe_failed: HashSet::new(),
            files_dirty_at: Some(Instant::now()),
            probe_tx,
            probe_rx,
            tasks: Vec::new(),
            library_filter: String::new(),
            collapsed_cats: Vec::new(),
            drag_kind: None,
            canvas_rect: egui::Rect::NOTHING,
            toolbar_rect: egui::Rect::NOTHING,
            panel_rects: PanelRects::default(),
            view_fitted: false,
            preview_index: 0,
            show_preview: true,
            show_help: false,
            toasts: Vec::new(),
            tools_error,
        };
        app.refresh_files();
        app
    }

    // -- screenshot / demo helpers ----------------------------------------

    /// Load a workflow file straight into the editor. Used by the screenshot
    /// harness so a specific graph (e.g. one with a Condition branch) can be
    /// rendered without a window.
    pub fn demo_load_workflow(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let text = std::fs::read_to_string(path)?;
        let wf = Workflow::from_json(&text)?;
        self.editor = graph::from_workflow(&wf, &mut self.ctx)?;
        self.workflow_path = Some(path.to_path_buf());
        self.root = workflow_root(&self.editor);
        self.mark_files_dirty();
        Ok(())
    }

    /// Set the canvas zoom the way the wheel does (through `PanZoom::zoom`, so
    /// the scaled style is rebuilt and the cards really are laid out at that
    /// zoom, not just translated).
    pub fn demo_set_zoom(&mut self, egui_ctx: &egui::Context, zoom: f32) {
        if (self.editor.pan_zoom.zoom - zoom).abs() < 1e-4 {
            return;
        }
        self.set_zoom(egui_ctx, zoom);
        // Rescaling around the centre leaves the graph off-screen when it was
        // not centred to begin with; recentre it so the shot shows the graph.
        self.center_graph();
    }

    /// Zoom the canvas without touching the pan.
    fn set_zoom(&mut self, egui_ctx: &egui::Context, zoom: f32) {
        let before = self.editor.pan_zoom.zoom;
        if (before - zoom).abs() < 1e-4 || before <= 0.0 {
            return;
        }
        let style = egui_ctx.style();
        let rect = self.canvas_rect;
        // `PanZoom::zoom` multiplies, so convert to a delta first.
        self.editor.pan_zoom.zoom(rect, &style, zoom / before);
        // Node positions are stored in *screen* pixels, and the editor rescales
        // them around the canvas centre whenever the zoom changes. Skipping
        // this (as a naive `zoom = z` assignment does) leaves the cards at the
        // old spacing while their contents scale — they overlap instantly.
        let delta = self.editor.pan_zoom.zoom / before;
        let half = self.editor.pan_zoom.clip_rect.size() / 2.0;
        let pan = self.editor.pan_zoom.pan;
        for pos in self.editor.node_positions.values_mut() {
            let local = pos.to_vec2() - half + pan;
            *pos = (local * delta).to_pos2() + half - pan;
        }
    }

    /// Bounding box of the cards, in the canvas-pixel units node positions are
    /// stored in — which are *screen* pixels, so the card footprint has to be
    /// scaled by the current zoom.
    fn graph_bbox(&self) -> egui::Rect {
        let zoom = self.editor.pan_zoom.zoom;
        let card = egui::vec2(EST_CARD_W, EST_CARD_H) * zoom;
        let mut bbox = egui::Rect::NOTHING;
        for pos in self.editor.node_positions.values() {
            bbox = bbox.union(egui::Rect::from_min_size(*pos, card));
        }
        bbox
    }

    /// Put the graph's centre in the middle of the canvas.
    fn center_graph(&mut self) {
        let rect = self.canvas_rect;
        let bbox = self.graph_bbox();
        if !bbox.is_finite() || bbox.is_negative() || !rect.is_finite() {
            return;
        }
        // Screen = position + pan + canvas.min.
        self.editor.pan_zoom.pan = rect.size() / 2.0 - bbox.center().to_vec2();
    }

    /// Fit the whole workflow into the canvas, once, on first layout.
    ///
    /// The default pipeline is wider than the centre panel on a 1080-wide
    /// window. Left at 100% the last cards sit *behind* the inspector, and a
    /// card that is chopped in half by a panel reads as a stacking bug rather
    /// than as "the graph continues off-screen". Fitting at startup shows the
    /// pipeline whole; the wheel and middle-drag take over from there.
    ///
    /// Returns `false` when there is nothing to fit yet (no canvas, no nodes),
    /// so the caller can retry on the next frame instead of giving up.
    fn fit_view(&mut self, egui_ctx: &egui::Context) -> bool {
        let canvas = self.canvas_rect;
        let bbox = self.graph_bbox();
        if !canvas.is_finite()
            || !bbox.is_finite()
            || bbox.width() <= 0.0
            || canvas.width() < 64.0
            || canvas.height() < 64.0
        {
            return false;
        }
        const PAD: f32 = 28.0;
        let room = egui::vec2(canvas.width() - PAD * 2.0, canvas.height() - PAD * 2.0);
        if room.x <= 0.0 || room.y <= 0.0 {
            return false;
        }
        // Both the card footprint and the spacing scale with zoom, so the ratio
        // between "what is there now" and "what would fit" is all we need.
        let ratio = (room.x / bbox.width()).min(room.y / bbox.height()).min(1.0);
        let zoom = self.editor.pan_zoom.zoom;
        let target = (zoom * ratio).clamp(MIN_FIT_ZOOM, 1.0);
        if (target - zoom).abs() > 0.005 {
            self.set_zoom(egui_ctx, target);
        }
        self.center_graph();
        true
    }

    /// Seed the UI with real data for the offscreen screenshot renderer
    /// (`--screenshot`): point the Source node at `dir`, resolve the file list,
    /// and pre-load a probe so the Demux inspector shows real tracks.
    pub fn demo_setup(
        &mut self,
        dir: Option<&std::path::Path>,
        probe_file: Option<&std::path::Path>,
    ) -> anyhow::Result<()> {
        if let Some(dir) = dir {
            self.root = Some(dir.to_path_buf());
            for node_id in graph::source_nodes(&self.editor.graph) {
                if let NodeConfig::Source(c) = &mut self.editor.graph.nodes[node_id].user_data.config
                {
                    c.paths = vec![dir.to_string_lossy().to_string()];
                }
            }
            self.refresh_files();
        }
        if let Some(f) = probe_file {
            let rt = tokio::runtime::Runtime::new()?;
            let info = rt.block_on(probe::probe(f))?;
            self.probes
                .insert(f.to_string_lossy().to_string(), info.clone());
            self.ctx.probe = Some(info);
        }
        Ok(())
    }

    /// Fill the task list with one job per status. Only used by `--screenshot
    /// --demo-tasks` so the task panel can be reviewed without a real batch.
    pub fn demo_tasks(&mut self) {
        let sources: Vec<PathBuf> = if self.files.is_empty() {
            vec![PathBuf::from("clip1.mp4"), PathBuf::from("clip2.mp4")]
        } else {
            self.files.clone()
        };
        self.tasks = sources
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let status = match i % 4 {
                    0 => TaskStatus::Done,
                    1 => TaskStatus::Running,
                    2 => TaskStatus::Failed,
                    _ => TaskStatus::Queued,
                };
                TaskRow {
                    id: i,
                    input: f.clone(),
                    output: f.with_file_name(format!("{}_fflow.mp4", i)),
                    status,
                    pct: match status {
                        TaskStatus::Done | TaskStatus::Failed => 100.0,
                        TaskStatus::Running => 42.0,
                        _ => 0.0,
                    },
                    speed: if status == TaskStatus::Running {
                        "1.9x".into()
                    } else {
                        String::new()
                    },
                    out_time: 1.3,
                    duration: 3.0,
                    error: if status == TaskStatus::Failed {
                        Some(
                            "ffmpeg exited with status 1\n\
                             [h264_nvenc @ 000001f2] OpenEncodeSessionEx failed: \
                             out of memory (10)\nConversion failed!"
                                .into(),
                        )
                    } else {
                        None
                    },
                    secs: if status == TaskStatus::Done { 1.2 } else { 0.0 },
                    expanded: status == TaskStatus::Failed,
                }
            })
            .collect();
    }

    /// Select the `index`-th node of the graph (document order).
    pub fn select_node(&mut self, index: usize) -> bool {
        match self.editor.node_order.get(index).copied() {
            Some(id) => {
                self.editor.selected_nodes = vec![id];
                true
            }
            None => false,
        }
    }

    // -- plumbing ---------------------------------------------------------

    /// Display name of the current workflow ("untitled" until saved).
    fn workflow_name(&self) -> String {
        self.workflow_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string())
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toasts.push(Toast {
            msg: msg.into(),
            born: Instant::now(),
            error: false,
        });
    }

    fn notify_err(&mut self, msg: impl Into<String>) {
        self.toasts.push(Toast {
            msg: msg.into(),
            born: Instant::now(),
            error: true,
        });
    }

    fn mark_files_dirty(&mut self) {
        self.files_dirty_at = Some(Instant::now());
    }

    /// Drop every piece of app state keyed by a node that no longer exists.
    ///
    /// The graph editor removes the node from `graph`/`node_order` itself, but
    /// the per-node caches living in `GraphCtx` and the file list derived from
    /// the Source nodes have to be cleaned up here, or the next frame keeps
    /// rendering from a node id that is gone.
    fn node_deleted(&mut self, json_id: u64, was_source: bool) {
        self.ctx.source_counts.remove(&json_id);
        graph::prune_dangling_connections(&mut self.editor.graph);
        if was_source {
            // `refresh_files` re-derives the list and clamps `preview_index`.
            self.mark_files_dirty();
        }
    }

    /// Remove the selected nodes.
    ///
    /// `egui_node_graph2` only does this for its own (10px) close button, which
    /// is easy to miss. This is the same sequence it runs, plus our cleanup.
    fn delete_selected(&mut self) {
        let victims = std::mem::take(&mut self.editor.selected_nodes);
        for node in victims {
            if self.editor.graph.nodes.get(node).is_none() {
                continue;
            }
            let (removed, _) = self.editor.graph.remove_node(node);
            self.editor.node_positions.remove(node);
            self.editor.node_order.retain(|n| *n != node);
            self.node_deleted(
                removed.user_data.json_id,
                removed.user_data.kind() == NodeKind::Source,
            );
        }
    }

    /// Keep the persisted panel sizes usable when the window is smaller than it
    /// was the last time they were dragged. Runs once per frame, right after the
    /// toolbar (whose height the side panels depend on).
    fn clamp_panels(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let p = &mut self.settings.panels;

        // Settings saved by an older build may sit outside today's ranges.
        p.library = p
            .library
            .clamp(*model::PanelSizes::LIBRARY.start(), *model::PanelSizes::LIBRARY.end());
        p.inspector = p
            .inspector
            .clamp(*model::PanelSizes::INSPECTOR.start(), *model::PanelSizes::INSPECTOR.end());
        p.tasks = p
            .tasks
            .clamp(*model::PanelSizes::TASKS.start(), *model::PanelSizes::TASKS.end());
        p.preview = p
            .preview
            .clamp(*model::PanelSizes::PREVIEW.start(), *model::PanelSizes::PREVIEW.end());
        p.files = p
            .files
            .clamp(*model::PanelSizes::FILES.start(), *model::PanelSizes::FILES.end());

        // Side panels first: they must leave room for the canvas.
        let room = (screen.width() - model::PanelSizes::MIN_CANVAS).max(0.0);
        if p.library + p.inspector > room {
            let slack = (p.library - *model::PanelSizes::LIBRARY.start()).max(0.0);
            p.library -= (p.library + p.inspector - room).min(slack);
            let slack = (p.inspector - *model::PanelSizes::INSPECTOR.start()).max(0.0);
            p.inspector -= (p.library + p.inspector - room).min(slack);
        }

        // Then the stacked bottom panels.
        let band = (screen.bottom() - self.toolbar_rect.bottom()).max(0.0);
        let room = (band - model::PanelSizes::MIN_CANVAS).max(0.0);
        if p.tasks + p.preview > room {
            let slack = (p.tasks - *model::PanelSizes::TASKS.start()).max(0.0);
            p.tasks -= (p.tasks + p.preview - room).min(slack);
            let slack = (p.preview - *model::PanelSizes::PREVIEW.start()).max(0.0);
            p.preview -= (p.tasks + p.preview - room).min(slack);
        }

        // The file list lives inside the library panel, so its ceiling is that
        // panel's height — not the screen's. Clamping against the screen let a
        // dragged file list grow straight through the node library and out the
        // top of the side panel.
        let library_h = if self.panel_rects.library_content.is_finite() {
            self.panel_rects.library_content.height()
        } else {
            (band - p.preview - p.tasks).max(0.0)
        };
        let max_files = (library_h - LIBRARY_MIN_NODES).max(*model::PanelSizes::FILES.start());
        p.files = p.files.min(max_files);
    }

    /// Keyboard shortcuts, handled before the panels so the effect lands on the
    /// same frame.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        // Never steal keys from a focused text field (search box, path editor…).
        if ctx.memory(|m| m.focused()).is_some() {
            return;
        }
        let (delete, escape) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if escape {
            self.editor.selected_nodes.clear();
        }
        if delete && !self.editor.selected_nodes.is_empty() {
            self.delete_selected();
        }
    }

    fn refresh_files(&mut self) {
        let mut all = Vec::new();
        for node_id in graph::source_nodes(&self.editor.graph) {
            if let NodeConfig::Source(cfg) = &self.editor.graph.nodes[node_id].user_data.config {
                let files = source::expand(
                    &cfg.paths,
                    cfg.recursive,
                    &cfg.extensions,
                    cfg.skip_own_outputs,
                );
                self.ctx
                    .source_counts
                    .insert(self.editor.graph.nodes[node_id].user_data.json_id, files.len());
                all.extend(files);
            }
        }
        // The Source node is the only place a folder is chosen, so the toolbar
        // has no picker any more: keep the cached root in step with the graph
        // for the file dialogs that start there.
        self.root = workflow_root(&self.editor);
        all.sort();
        all.dedup();
        if self.preview_index >= all.len() {
            self.preview_index = 0;
        }
        self.files = all;
        self.files_dirty_at = None;
        self.probe_failed.clear();
    }

    fn ensure_probes(&mut self) {
        let mut wanted: Vec<PathBuf> = Vec::new();
        if let Some(f) = self.files.get(self.preview_index) {
            wanted.push(f.clone());
        }
        wanted.extend(self.files.iter().take(64).cloned());

        for path in wanted {
            let key = path.to_string_lossy().to_string();
            if self.probes.contains_key(&key)
                || self.probe_inflight.contains(&key)
                || self.probe_failed.contains(&key)
            {
                continue;
            }
            self.probe_inflight.insert(key);
            self.runner.spawn_probe(path, self.probe_tx.clone());
        }
    }

    fn poll_probes(&mut self) {
        while let Ok(msg) = self.probe_rx.try_recv() {
            let key = msg.path.to_string_lossy().to_string();
            self.probe_inflight.remove(&key);
            match msg.result {
                Ok(info) => {
                    self.probes.insert(key, info);
                }
                Err(e) => {
                    self.probe_failed.insert(key);
                    self.notify_err(format!("probe failed: {}", e));
                }
            }
        }
        // Feed the Demux "port lit" state from the previewed file.
        self.ctx.probe = self
            .files
            .get(self.preview_index)
            .and_then(|f| self.probes.get(&f.to_string_lossy().to_string()))
            .cloned();
    }

    fn poll_runner(&mut self) {
        for ev in self.runner.poll() {
            match ev {
                RunEvent::Started { id } => {
                    if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                        t.status = TaskStatus::Running;
                    }
                }
                RunEvent::Progress {
                    id,
                    pct,
                    out_time,
                    speed,
                } => {
                    if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                        t.pct = pct;
                        t.out_time = out_time;
                        t.speed = speed;
                    }
                }
                RunEvent::Finished {
                    id,
                    ok,
                    cancelled,
                    error,
                    secs,
                    output,
                } => {
                    if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
                        t.status = if cancelled {
                            TaskStatus::Cancelled
                        } else if ok {
                            TaskStatus::Done
                        } else {
                            TaskStatus::Failed
                        };
                        if ok {
                            t.pct = 100.0;
                        }
                        t.error = error;
                        t.secs = secs;
                        t.output = output;
                        if t.status == TaskStatus::Failed {
                            t.expanded = true;
                        }
                    }
                }
                RunEvent::BatchDone {
                    ok,
                    failed,
                    cancelled,
                } => {
                    self.notify(format!(
                        "batch finished — {} ok, {} failed, {} cancelled",
                        ok, failed, cancelled
                    ));
                }
            }
        }
    }

    fn handle_dropped(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let mut paths: Vec<String> = Vec::new();
        for f in dropped {
            if let Some(p) = f.path {
                if p.is_dir() {
                    if self.root.is_none() {
                        self.root = Some(p.clone());
                    }
                }
                paths.push(p.to_string_lossy().to_string());
            }
        }
        if paths.is_empty() {
            return;
        }
        // Attach to the selected Source node, else the first one, else create one.
        let target = self
            .editor
            .selected_nodes
            .iter()
            .copied()
            .find(|n| self.editor.graph.nodes[*n].user_data.kind() == NodeKind::Source)
            .or_else(|| graph::source_nodes(&self.editor.graph).first().copied());

        let target = match target {
            Some(t) => t,
            None => {
                let pos = self.canvas_rect.center() - self.canvas_rect.min.to_vec2()
                    - self.editor.pan_zoom.pan;
                graph::add_node(&mut self.editor, &mut self.ctx, NodeKind::Source, pos)
            }
        };
        if let Some(node) = self.editor.graph.nodes.get_mut(target) {
            if let NodeConfig::Source(cfg) = &mut node.user_data.config {
                for p in paths {
                    if !cfg.paths.contains(&p) {
                        cfg.paths.push(p);
                    }
                }
            }
        }
        self.notify("added dropped path(s) to the Source node");
        self.mark_files_dirty();
    }

    // -- compile / run ----------------------------------------------------

    fn compile_current(&self) -> Result<Job, String> {
        let Some(file) = self.files.get(self.preview_index) else {
            return Err("no input file — pick a root folder first".into());
        };
        let key = file.to_string_lossy().to_string();
        let Some(info) = self.probes.get(&key) else {
            return Err(if self.probe_failed.contains(&key) {
                format!("{} could not be read by ffprobe", source::short_name(file))
            } else {
                "probing…".into()
            });
        };
        compiler::compile(
            &self.editor.graph,
            file,
            info,
            self.preview_index + 1,
            self.root.as_deref(),
        )
        .map_err(|e| match compiler::skipped_reason(&e) {
            Some(why) => format!("skipped — {}", why),
            None => format!("{:#}", e),
        })
    }

    fn start_run(&mut self) {
        if self.runner.is_running() {
            return;
        }
        if self.files.is_empty() {
            self.notify_err("no input files — choose a root folder first");
            return;
        }
        // A file ffprobe already rejected is not "still probing": waiting for
        // it would block the batch forever.
        let pending = self
            .files
            .iter()
            .filter(|f| {
                let key = f.to_string_lossy();
                !self.probes.contains_key(key.as_ref()) && !self.probe_failed.contains(key.as_ref())
            })
            .count();
        if pending > 0 {
            self.notify_err(format!("still probing {} file(s), try again in a moment", pending));
            self.ensure_probes();
            return;
        }

        let mut jobs = Vec::new();
        let mut errors = Vec::new();
        let mut skipped = 0usize;
        for (i, file) in self.files.clone().iter().enumerate() {
            let key = file.to_string_lossy().to_string();
            let Some(info) = self.probes.get(&key).cloned() else {
                errors.push(format!(
                    "{}: unreadable — ffprobe could not open it",
                    source::short_name(file)
                ));
                continue;
            };
            match compiler::compile(&self.editor.graph, file, &info, i + 1, self.root.as_deref()) {
                Ok(job) => jobs.push(job),
                // A Condition routed this file to an empty branch: not an error.
                Err(e) if compiler::skipped_reason(&e).is_some() => skipped += 1,
                Err(e) => errors.push(format!("{}: {:#}", source::short_name(file), e)),
            }
        }

        if jobs.is_empty() {
            self.notify_err(
                errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "nothing to run".into()),
            );
            return;
        }

        self.tasks = jobs
            .iter()
            .map(|j| TaskRow {
                id: j.id,
                input: j.input.clone(),
                output: j.output.clone(),
                status: TaskStatus::Queued,
                pct: 0.0,
                speed: String::new(),
                out_time: 0.0,
                duration: j.duration,
                error: None,
                secs: 0.0,
                expanded: false,
            })
            .collect();
        for e in &errors {
            self.notify_err(e.clone());
        }
        if skipped > 0 {
            self.notify(format!("{} file(s) skipped by a condition", skipped));
        }
        self.runner.start(jobs, self.settings.concurrency);
        self.notify(format!("started {} job(s)", self.tasks.len()));
    }

    fn save_workflow(&mut self) {
        let wf = graph::to_workflow(&self.editor);
        let path = self
            .workflow_path
            .clone()
            .or_else(|| source::pick_workflow_save(self.root.as_deref()));
        let Some(path) = path else { return };
        match wf.to_json().and_then(|s| {
            std::fs::write(&path, s)?;
            Ok(())
        }) {
            Ok(()) => {
                self.workflow_path = Some(path.clone());
                self.settings.last_workflow = path.to_string_lossy().to_string();
                save_settings(&self.settings);
                self.notify(format!("saved {}", path.display()));
            }
            Err(e) => self.notify_err(format!("save failed: {:#}", e)),
        }
    }

    fn save_workflow_as(&mut self) {
        if let Some(path) = source::pick_workflow_save(self.root.as_deref()) {
            self.workflow_path = Some(path);
            self.save_workflow();
        }
    }

    fn load_workflow(&mut self) {
        let Some(path) = source::pick_workflow_open(self.root.as_deref()) else {
            return;
        };
        match std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .and_then(|s| Workflow::from_json(&s))
            .and_then(|wf| graph::from_workflow(&wf, &mut self.ctx))
        {
            Ok(editor) => {
                self.editor = editor;
                self.workflow_path = Some(path.clone());
                self.settings.last_workflow = path.to_string_lossy().to_string();
                self.root = workflow_root(&self.editor);
                self.mark_files_dirty();
                self.notify(format!("loaded {}", path.display()));
            }
            Err(e) => self.notify_err(format!("load failed: {:#}", e)),
        }
    }

    fn new_workflow(&mut self) {
        self.ctx = GraphCtx::default();
        let wf = graph::default_workflow(self.root.as_deref());
        match graph::from_workflow(&wf, &mut self.ctx) {
            Ok(editor) => {
                self.editor = editor;
                self.workflow_path = None;
                self.tasks.clear();
                self.mark_files_dirty();
            }
            Err(e) => self.notify_err(format!("{:#}", e)),
        }
    }

    // -- panels -----------------------------------------------------------

    fn toolbar(&mut self, ctx: &egui::Context) {
        let pal = theme::p();
        let bar = egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::none()
                    .fill(pal.panel)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
            )
            .show(ctx, |ui| {
                let mut narrow = false;
                ui.horizontal(|ui| {
                    // -- brand + workflow name ------------------------------
                    ui.label(
                        egui::RichText::new("fflow")
                            .strong()
                            .size(theme::TITLE)
                            .color(pal.accent),
                    );
                    let name = self.workflow_name();
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("· {name}")).size(theme::BODY).color(pal.dim),
                        )
                        .truncate(),
                    );

                    ui.add_space(6.0);
                    toolbar_divider(ui);
                    ui.add_space(2.0);

                    // -- run / cancel --------------------------------------
                    let running = self.runner.is_running();
                    ui.add_enabled_ui(!running, |ui| {
                        if theme::primary_button(ui, "▶  Run")
                            .on_hover_text("Compile every file and run the batch")
                            .clicked()
                        {
                            self.start_run();
                        }
                    });
                    ui.add_enabled_ui(running, |ui| {
                        if theme::danger_button(ui, "■  Cancel").clicked() {
                            self.runner.cancel_all();
                            self.notify("cancelling…");
                        }
                    });

                    ui.add_space(6.0);
                    toolbar_divider(ui);
                    ui.add_space(2.0);

                    // -- workflow files ------------------------------------
                    if theme::ghost_button(ui, "Save")
                        .on_hover_text("Save workflow")
                        .clicked()
                    {
                        self.save_workflow();
                    }
                    if theme::ghost_button(ui, "Save as…")
                        .on_hover_text("Save workflow to a new JSON file")
                        .clicked()
                    {
                        self.save_workflow_as();
                    }
                    if theme::ghost_button(ui, "Load")
                        .on_hover_text("Load workflow")
                        .clicked()
                    {
                        self.load_workflow();
                    }
                    if theme::ghost_button(ui, "New")
                        .on_hover_text("New workflow from the default template")
                        .clicked()
                    {
                        self.new_workflow();
                    }

                    // -- right cluster -------------------------------------
                    // Right-aligned, but only if there is room: on a narrow
                    // window it moves to a second toolbar row instead of
                    // overlapping the buttons on the left.
                    narrow = ui.available_width() < 430.0;
                    if !narrow {
                        self.toolbar_right(ui);
                    }
                });
                if narrow {
                    ui.horizontal(|ui| self.toolbar_right(ui));
                }
            });
        self.toolbar_rect = bar.response.rect;
    }

    /// Right-hand toolbar cluster: help, palette, preview toggle, concurrency
    /// and the encoder/file pills.
    fn toolbar_right(&mut self, ui: &mut egui::Ui) {
        let pal = theme::p();
        theme::right_aligned(ui, |ui| {
            if theme::ghost_button(ui, "?").on_hover_text("Help").clicked() {
                self.show_help = !self.show_help;
            }
            let theme_label = if self.settings.dark_mode {
                "Dark"
            } else {
                "Light"
            };
            if theme::ghost_button(ui, theme_label)
                .on_hover_text("Switch between the dark and light palettes")
                .clicked()
            {
                self.settings.dark_mode = !self.settings.dark_mode;
                theme::apply(ui.ctx(), self.settings.dark_mode);
                save_settings(&self.settings);
            }
            let preview_label = if self.show_preview {
                "Preview [on]"
            } else {
                "Preview [off]"
            };
            if theme::ghost_button(ui, preview_label)
                .on_hover_text("Show the generated ffmpeg command")
                .clicked()
            {
                self.show_preview = !self.show_preview;
            }
            ui.add_space(2.0);
            // A nested horizontal scope keeps "Parallel [2]" in
            // reading order inside the right-to-left layout.
            ui.scope(|ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.settings.concurrency)
                            .range(1..=16)
                            .speed(0.1),
                    )
                    .on_hover_text(
                        "How many files are processed at the same time",
                    );
                    ui.label(
                        egui::RichText::new("Parallel").size(theme::BODY).color(pal.dim),
                    );
                });
            });
            ui.add_space(2.0);
            if let Some(err) = &self.tools_error {
                theme::pill(ui, "ffmpeg missing", pal.err)
                    .on_hover_text(err.clone());
            }
            if crate::encoders::is_detecting() {
                theme::pill(ui, "detecting…", pal.warn);
            } else if let Some(reg) = crate::encoders::snapshot() {
                let hw = reg.working_hardware();
                if hw.is_empty() {
                    theme::pill(ui, "software encode", pal.faint)
                        .on_hover_text(reg.summary());
                } else {
                    let text = if hw.len() == 1 {
                        format!("⚡ {}", hw[0])
                    } else {
                        format!("⚡ {} +{}", hw[0], hw.len() - 1)
                    };
                    theme::pill(ui, text, pal.ok).on_hover_text(reg.summary());
                }
            }
            theme::pill(ui, format!("{} files", self.files.len()), pal.file);
        });
    }

    fn library(&mut self, ctx: &egui::Context) {
        let pal = theme::p();
        let panel = egui::SidePanel::left("library")
            // Sized by our own splitter, not by egui: the canvas registers a
            // click+drag background rect in the same layer *after* the panels,
            // so egui's handle is dead wherever it touches the canvas.
            .resizable(false)
            .exact_width(self.settings.panels.library)
            .frame(
                egui::Frame::none()
                    .fill(pal.panel)
                    .inner_margin(egui::Margin::symmetric(8.0, 8.0)),
            )
            .show(ctx, |ui| {
                self.panel_rects.library_content = ui.max_rect();
                // The panel's clip rect is the whole panel, which is wider than
                // this frame's content box. A scroll area pushed to that clip
                // edge (egui does that when the clip is wider than the area)
                // then overhangs by the frame margin, and the panel's *response*
                // rect grows with it: the panel ended up 12 px wider than
                // `exact_width`, leaving an unfilled strip that showed the
                // window's black clear colour. Clipping to our own box keeps
                // every child inside it.
                ui.set_clip_rect(ui.max_rect());
                // Files discovered by the Source node, below the library.
                let files = egui::TopBottomPanel::bottom("file_list")
                    .resizable(false)
                    .exact_height(self.settings.panels.files)
                    .frame(
                        egui::Frame::none()
                            .inner_margin(egui::Margin { top: 6.0, ..egui::Margin::ZERO }),
                    )
                    .show_inside(ui, |ui| {
                        self.file_list(ui);
                    });
                self.panel_rects.files = files.response.rect;

                ui.label(
                    egui::RichText::new("NODE LIBRARY")
                        .size(theme::CAPTION)
                        .strong()
                        .color(pal.faint),
                );
                ui.add_space(3.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.library_filter)
                        .hint_text("search nodes…")
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("Click a node to add it, or drag it onto the canvas");

                let filter = self.library_filter.trim().to_ascii_lowercase();
                // Group up front so empty categories disappear while searching.
                let mut groups: Vec<(&'static str, Vec<NodeKind>)> = Vec::new();
                for kind in NodeKind::ALL {
                    if !kind.matches(&filter) {
                        continue;
                    }
                    match groups.iter_mut().find(|(c, _)| *c == kind.category()) {
                        Some((_, v)) => v.push(kind),
                        None => groups.push((kind.category(), vec![kind])),
                    }
                }

                let mut add_kind: Option<NodeKind> = None;
                // Stop a few pixels short of the divider: the list is clipped at
                // its bottom edge, and without the gutter the splitter line was
                // drawn straight through the middle of the last visible card.
                let list_h = (ui.available_height() - LIBRARY_GUTTER).max(0.0);
                let list = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(list_h)
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                    )
                    .show(ui, |ui| {
                        if groups.is_empty() {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!("no card matches “{filter}”"))
                                    .size(theme::CAPTION)
                                    .color(pal.faint),
                            );
                            return;
                        }
                        for (cat, kinds) in &groups {
                            // A search always expands the groups it matched;
                            // otherwise the user's own fold state wins.
                            let open = !filter.is_empty()
                                || !self.collapsed_cats.iter().any(|c| c == cat);
                            let resp = category_header(ui, cat, kinds.len(), open);
                            if resp.clicked() && filter.is_empty() {
                                if let Some(pos) =
                                    self.collapsed_cats.iter().position(|c| c == cat)
                                {
                                    self.collapsed_cats.remove(pos);
                                } else {
                                    self.collapsed_cats.push((*cat).to_string());
                                }
                            }
                            if !open {
                                continue;
                            }
                            for kind in kinds {
                                let resp = theme::card(
                                    ui,
                                    false,
                                    library_card_id(*kind),
                                    egui::Sense::click_and_drag(),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            theme::accent_bar(
                                                ui,
                                                theme::node_color(*kind),
                                                3.0,
                                                16.0,
                                            );
                                            ui.add_space(2.0);
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{}  {}",
                                                    icon_for(*kind),
                                                    kind.title()
                                                ))
                                                .size(theme::BODY)
                                                .color(pal.text),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(kind.blurb())
                                                        .size(theme::CAPTION)
                                                        .color(pal.faint),
                                                )
                                                .truncate(),
                                            );
                                        });
                                    },
                                )
                                .on_hover_text(format!(
                                    "{}\n{}",
                                    kind.blurb(),
                                    kind.keywords()
                                ));
                                if resp.clicked() {
                                    add_kind = Some(*kind);
                                }
                                if resp.drag_started() {
                                    self.drag_kind = Some(*kind);
                                }
                                ui.add_space(1.0);
                            }
                        }
                    });
                theme::fade_bottom(ui, list.inner_rect, 18.0);

                if let Some(kind) = add_kind {
                    let pos = self.canvas_rect.center() - self.canvas_rect.min.to_vec2()
                        - self.editor.pan_zoom.pan;
                    graph::add_node(&mut self.editor, &mut self.ctx, kind, pos);
                }
            });
        self.panel_rects.library = panel.response.rect;
    }

    /// List of files the Source node resolves to, with whatever ffprobe knows.
    fn file_list(&mut self, ui: &mut egui::Ui) {
        let pal = theme::p();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("FILES")
                    .size(theme::CAPTION)
                    .strong()
                    .color(pal.faint),
            );
            ui.add_space(4.0);
            theme::pill(ui, format!("{}", self.files.len()), pal.file);
            theme::right_aligned(ui, |ui| {
                if theme::ghost_button(ui, "rescan")
                    .on_hover_text("Re-scan the Source node")
                    .clicked()
                {
                    self.mark_files_dirty();
                    self.refresh_files();
                }
            });
        });
        ui.add_space(2.0);
        if self.files.is_empty() {
            theme::empty_state(
                ui,
                "📁",
                "No files yet",
                "Pick a root folder to fill the queue",
            );
            return;
        }
        let mut pick: Option<usize> = None;
        let list = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, f) in self.files.iter().enumerate() {
                    let selected = i == self.preview_index;
                    let probe = self.probes.get(&f.to_string_lossy().to_string());
                    let resp = theme::card(
                        ui,
                        selected,
                        file_card_id(i),
                        egui::Sense::click(),
                        |ui| {
                            // Two truncating labels side by side: the first one
                            // would eat the whole row and leave the second a
                            // zero-width slot, whose minimum size then pushed
                            // the row 12 px past the card. Splitting the width
                            // explicitly keeps both inside it, and the name is
                            // laid out left-to-right (`add_sized` would centre
                            // it in its column).
                            let row = ui.available_width();
                            let name_w = (row * 0.58).max(0.0);
                            let meta_w = (row - name_w - ui.spacing().item_spacing.x).max(0.0);
                            let line = ui.spacing().interact_size.y;
                            ui.horizontal(|ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(name_w, line),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        // `allocate_ui_with_layout` only
                                        // reserves what the child used, so the
                                        // column has to claim its width or a
                                        // short name pulls the meta left.
                                        ui.set_min_width(name_w);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(source::short_name(f))
                                                    .size(theme::BODY)
                                                    .color(pal.text),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                                let meta = match probe {
                                    Some(p) => p.streams_summary(),
                                    None => "probing…".to_string(),
                                };
                                ui.allocate_ui_with_layout(
                                    egui::vec2(meta_w, line),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_width(meta_w);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(meta)
                                                    .size(theme::CAPTION)
                                                    .color(pal.faint),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                            });
                        },
                    )
                    .on_hover_text(f.display().to_string());
                    if resp.clicked() {
                        pick = Some(i);
                    }
                    ui.add_space(2.0);
                }
            });
        theme::fade_bottom(ui, list.inner_rect, 16.0);
        if let Some(i) = pick {
            self.preview_index = i;
        }
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        let pal = theme::p();
        let panel = egui::SidePanel::right("inspector")
            .resizable(false)
            .exact_width(self.settings.panels.inspector)
            .frame(
                egui::Frame::none()
                    .fill(pal.panel)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("INSPECTOR")
                        .size(theme::CAPTION)
                        .strong()
                        .color(pal.faint),
                );
                ui.add_space(4.0);

                let selected = self.editor.selected_nodes.clone();
                match selected.len() {
                    0 => {
                        theme::empty_state(
                            ui,
                            "⚙",
                            "Nothing selected",
                            "Click a node on the canvas to edit it",
                        );
                    }
                    1 => {
                        let node_id = selected[0];
                        let Some(node) = self.editor.graph.nodes.get(node_id) else {
                            return;
                        };
                        let kind = node.user_data.kind();
                        let mut cfg = node.user_data.config.clone();
                        let cat = theme::node_color(kind);

                        // -- node header -----------------------------------
                        let mut delete_me = false;
                        ui.horizontal(|ui| {
                            theme::accent_bar(ui, cat, 3.0, 34.0);
                            ui.add_space(2.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}  {}",
                                        icon_for(kind),
                                        kind.title()
                                    ))
                                    .size(theme::TITLE)
                                    .strong()
                                    .color(pal.text),
                                );
                                ui.label(
                                    egui::RichText::new(kind.category().to_uppercase())
                                        .size(theme::CAPTION)
                                        .strong()
                                        .color(cat),
                                );
                            });
                            theme::right_aligned(ui, |ui| {
                                if theme::danger_button(ui, "Delete")
                                    .on_hover_text("Remove this node (Del)")
                                    .clicked()
                                {
                                    delete_me = true;
                                }
                            });
                        });
                        if delete_me {
                            self.delete_selected();
                            return;
                        }
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(kind.blurb()).size(theme::SMALL).color(pal.faint),
                        );
                        theme::rule(ui);

                        // -- form ------------------------------------------
                        // Borrow the probe and the encoder registry before
                        // touching `self` mutably again.
                        let probe = self.ctx.probe.clone();
                        let reg = crate::encoders::snapshot();
                        // Leave room for the "compiled output" block below
                        // instead of taking a fixed percentage: the block is
                        // only a few lines, so the form gets the rest.
                        egui::ScrollArea::vertical()
                            .id_salt("inspector-form")
                            .auto_shrink([false, true])
                            .max_height((ui.available_height() - 118.0).max(140.0))
                            .show(ui, |ui| {
                                let outcome = nodes::inspector_ui(
                                    &mut cfg,
                                    ui,
                                    probe.as_ref(),
                                    reg.as_deref(),
                                );
                                if outcome.changed {
                                    if let Some(node) = self.editor.graph.nodes.get_mut(node_id) {
                                        node.user_data.config = cfg;
                                    }
                                    self.mark_files_dirty();
                                }
                                if outcome.request_probe {
                                    if let Some(file) = self.files.first().cloned() {
                                        self.probes
                                            .remove(&file.to_string_lossy().to_string());
                                        self.ensure_probes();
                                    }
                                }
                                if outcome.redetect {
                                    crate::encoders::redetect();
                                    self.notify("re-detecting encoders…");
                                }
                            });

                        // -- compiled result for this node -----------------
                        theme::rule(ui);
                        theme::section(ui, "Compiled output");
                        let job = self.compile_current();
                        match job {
                            Ok(job) => {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(source::short_name(&job.input))
                                            .size(theme::SMALL)
                                            .color(pal.dim),
                                    );
                                    ui.label(
                                        egui::RichText::new("»")
                                            .size(theme::SMALL)
                                            .color(pal.faint),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(source::short_name(&job.output))
                                                .size(theme::SMALL)
                                                .color(pal.ok),
                                        )
                                        .truncate(),
                                    );
                                });
                                for n in &job.notes {
                                    ui.label(
                                        egui::RichText::new(format!("· {}", n))
                                            .size(theme::SMALL)
                                            .color(pal.faint),
                                    );
                                }
                            }
                            Err(e) => {
                                ui.colored_label(
                                    pal.warn,
                                    egui::RichText::new(e).size(theme::SMALL),
                                );
                            }
                        }
                    }
                    n => {
                        theme::empty_state(
                            ui,
                            "▣",
                            &format!("{n} nodes selected"),
                            "Select a single node to edit its properties",
                        );
                    }
                }
            });
        self.panel_rects.inspector = panel.response.rect;
    }
    fn task_panel(&mut self, ctx: &egui::Context) {
        let pal = theme::p();
        let panel = egui::TopBottomPanel::bottom("tasks")
            .resizable(false)
            .exact_height(self.settings.panels.tasks)
            .frame(
                egui::Frame::none()
                    .fill(pal.panel)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
            )
            .show(ctx, |ui| {
                let total = self.tasks.len();
                let running = self
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Running)
                    .count();
                let done = self
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Done)
                    .count();
                let failed = self
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Failed)
                    .count();
                let cancelled = self
                    .tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Cancelled)
                    .count();

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("TASKS")
                            .size(theme::CAPTION)
                            .strong()
                            .color(pal.faint),
                    );
                    if total == 0 {
                        ui.label(
                            egui::RichText::new("no jobs yet")
                                .size(theme::SMALL)
                                .color(pal.faint),
                        );
                    } else {
                        if running > 0 {
                            theme::pill(ui, format!("{} running", running), pal.accent);
                        }
                        if done > 0 {
                            theme::pill(ui, format!("{} done", done), pal.ok);
                        }
                        if failed > 0 {
                            theme::pill(ui, format!("{} failed", failed), pal.err);
                        }
                        if cancelled > 0 {
                            theme::pill(ui, format!("{} cancelled", cancelled), pal.warn);
                        }
                    }

                    theme::right_aligned(ui, |ui| {
                        if total > 0 && theme::ghost_button(ui, "Clear").clicked() {
                            self.tasks.clear();
                        }
                        if total > 0 {
                            let overall: f32 =
                                self.tasks.iter().map(|t| t.pct).sum::<f32>() / total as f32;
                            let color = if failed > 0 {
                                pal.err
                            } else if done == total {
                                pal.ok
                            } else {
                                pal.accent
                            };
                            theme::progress(
                                ui,
                                170.0,
                                overall,
                                color,
                                format!("{:.0}% overall", overall),
                            );
                        }
                    });
                });

                if total == 0 {
                    theme::empty_state(
                        ui,
                        "▶",
                        "Nothing running",
                        "Press Run to process every file found by the Source node",
                    );
                    return;
                }

                theme::rule(ui);
                // Column headers, matching the row layout below.
                ui.horizontal(|ui| {
                    let head = |ui: &mut egui::Ui, text: &str, w: f32| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(w, 14.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(w);
                                ui.label(
                                    egui::RichText::new(text)
                                        .size(theme::CAPTION)
                                        .strong()
                                        .color(pal.faint),
                                );
                            },
                        );
                    };
                    head(ui, "", 12.0);
                    head(ui, "FILE", 240.0);
                    head(ui, "PROGRESS", 200.0);
                    head(ui, "STATUS", 70.0);
                    head(ui, "DETAIL", 220.0);
                });

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut open_folder: Option<PathBuf> = None;
                        for task in &mut self.tasks {
                            let row_h = 20.0;
                            let cell = |ui: &mut egui::Ui, w: f32, add: &dyn Fn(&mut egui::Ui)| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(w, row_h),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_width(w);
                                        add(ui);
                                    },
                                );
                            };
                            ui.horizontal(|ui| {
                                cell(ui, 12.0, &|ui| {
                                    ui.label(
                                        egui::RichText::new("•")
                                            .size(theme::DOT)
                                            .color(task.status.color()),
                                    );
                                });
                                cell(ui, 240.0, &|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(source::short_name(&task.input))
                                                .size(theme::BODY)
                                                .color(pal.text),
                                        )
                                        .truncate(),
                                    );
                                });
                                theme::progress(
                                    ui,
                                    200.0,
                                    task.pct,
                                    task.status.color(),
                                    format!("{:.0}%", task.pct),
                                );
                                cell(ui, 70.0, &|ui| {
                                    ui.label(
                                        egui::RichText::new(task.status.label())
                                            .size(theme::SMALL)
                                            .color(task.status.color()),
                                    );
                                });
                                cell(ui, 220.0, &|ui| {
                                    let detail = if task.status == TaskStatus::Running {
                                        if task.speed.is_empty() {
                                            format!("{:.1}s / {:.1}s", task.out_time, task.duration)
                                        } else {
                                            format!(
                                                "{} · {:.1}s / {:.1}s",
                                                task.speed, task.out_time, task.duration
                                            )
                                        }
                                    } else if task.secs > 0.0 {
                                        format!("{:.1}s", task.secs)
                                    } else {
                                        String::new()
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(detail)
                                                .size(theme::SMALL)
                                                .color(pal.faint),
                                        )
                                        .truncate(),
                                    );
                                });

                                if task.error.is_some() {
                                    let label = if task.expanded { "hide" } else { "error" };
                                    if theme::ghost_button(ui, label).clicked() {
                                        task.expanded = !task.expanded;
                                    }
                                }
                                if matches!(
                                    task.status,
                                    TaskStatus::Done | TaskStatus::Failed
                                ) && theme::ghost_button(ui, "open")
                                    .on_hover_text("Reveal the output folder")
                                    .clicked()
                                {
                                    open_folder =
                                        task.output.parent().map(|p| p.to_path_buf());
                                }
                            });
                            if task.expanded {
                                if let Some(err) = &task.error {
                                    egui::Frame::none()
                                        .fill(pal.bg)
                                        .rounding(egui::Rounding::same(6.0))
                                        .inner_margin(egui::Margin::same(8.0))
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(err)
                                                        .monospace()
                                                        .size(theme::SMALL)
                                                        .color(pal.err),
                                                )
                                                // Labels are globally non-selectable so
                                                // node titlebars stay draggable; ffmpeg's
                                                // stderr is the one thing people do want
                                                // to copy out.
                                                .selectable(true)
                                                .wrap(),
                                            );
                                        });
                                    ui.add_space(2.0);
                                }
                            }
                        }
                        if let Some(dir) = open_folder {
                            reveal_in_explorer(&dir);
                        }
                    });
            });
        self.panel_rects.tasks = panel.response.rect;
    }

    fn preview_panel(&mut self, ctx: &egui::Context) {
        if !self.show_preview {
            // No panel, no edge: without this the `split-preview` handle would
            // stay where the panel used to be and drag nothing.
            self.panel_rects.preview = egui::Rect::NOTHING;
            return;
        }
        let pal = theme::p();
        let panel = egui::TopBottomPanel::bottom("preview")
            .resizable(false)
            .exact_height(self.settings.panels.preview)
            .frame(
                egui::Frame::none()
                    .fill(pal.panel)
                    .inner_margin(egui::Margin::symmetric(10.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("COMMAND PREVIEW")
                            .size(theme::CAPTION)
                            .strong()
                            .color(pal.faint),
                    );

                    if !self.files.is_empty() {
                        if theme::ghost_button(ui, "◀").clicked() && self.preview_index > 0 {
                            self.preview_index -= 1;
                        }
                        let current = self
                            .files
                            .get(self.preview_index)
                            .map(|f| source::short_name(f))
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("preview-file")
                            .selected_text(current)
                            .width(230.0)
                            .show_ui(ui, |ui| {
                                for (i, f) in self.files.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            i == self.preview_index,
                                            source::short_name(f),
                                        )
                                        .clicked()
                                    {
                                        self.preview_index = i;
                                    }
                                }
                            });
                        if theme::ghost_button(ui, "▶").clicked()
                            && self.preview_index + 1 < self.files.len()
                        {
                            self.preview_index += 1;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{}/{}",
                                self.preview_index + 1,
                                self.files.len()
                            ))
                            .size(theme::SMALL)
                            .color(pal.faint),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("no files")
                                .size(theme::SMALL)
                                .color(pal.faint),
                        );
                    }

                    theme::right_aligned(ui, |ui| {
                        if theme::ghost_button(ui, "Copy")
                            .on_hover_text("Copy the full command line")
                            .clicked()
                        {
                            if let Ok(job) = self.compile_current() {
                                ui.ctx().copy_text(job.display.clone());
                                self.notify("command copied");
                            }
                        }
                    });
                });

                // The panel has a fixed height, so long filtergraphs have to
                // scroll instead of being clipped mid-argument. The bar is
                // always visible: without it a half-line at the panel edge
                // looks like a paint bug rather than "there is more".
                egui::ScrollArea::vertical()
                    .id_salt("preview-body")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                    )
                    .show(ui, |ui| {
                        match self.compile_current() {
                            Ok(job) => {
                                ui.add_space(2.0);
                                // The output path is the command's last
                                // argument, so the redundant row is the first
                                // thing to drop when the panel is too short to
                                // show the whole command.
                                if self.settings.panels.preview >= 140.0 {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("OUTPUT")
                                                .size(theme::CAPTION)
                                                .strong()
                                                .color(pal.faint),
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(
                                                    job.output.display().to_string(),
                                                )
                                                .size(theme::SMALL)
                                                .color(pal.ok),
                                            )
                                            .truncate(),
                                        );
                                    });
                                    ui.add_space(2.0);
                                }
                                egui::Frame::none()
                                    .fill(pal.bg)
                                    .rounding(egui::Rounding::same(6.0))
                                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 5.0;
                                            ui.label(
                                                egui::RichText::new(crate::probe::ffmpeg_bin())
                                                    .monospace()
                                                    .size(theme::BODY)
                                                    .color(pal.dim),
                                            );
                                            let n = job.args.len();
                                            let mut prev: Option<&str> = None;
                                            for (i, arg) in job.args.iter().enumerate() {
                                                let color = command_token_color(arg, prev, i + 1 == n);
                                                ui.label(
                                                    egui::RichText::new(arg)
                                                        .monospace()
                                                        .size(theme::BODY)
                                                        .color(color),
                                                );
                                                prev = Some(arg.as_str());
                                            }
                                        });
                                    });
                                if !job.notes.is_empty() {
                                    ui.add_space(2.0);
                                    ui.horizontal_wrapped(|ui| {
                                        for n in &job.notes {
                                            theme::pill(ui, n.clone(), pal.faint);
                                        }
                                    });
                                }
                            }
                            Err(e) => {
                                ui.add_space(6.0);
                                ui.colored_label(pal.warn, egui::RichText::new(e).monospace().size(theme::BODY));
                            }
                        }
                    });
            });
        self.panel_rects.preview = panel.response.rect;
    }

    /// Drag one splitter strip and return the pointer coordinate along its axis.
    ///
    /// The strips are drawn as `Area`s in their own layer *after* the canvas, so
    /// they win the pointer even though they straddle the panel edge. That is the
    /// whole point: egui's own panel handles are registered before the canvas and
    /// the canvas claims every drag that starts inside its rect, which is why the
    /// built-in handles were dead on the two edges that touch the canvas.
    fn splitters(&mut self, ctx: &egui::Context) {
        let rects = self.panel_rects;
        let sizes = &mut self.settings.panels;

        // First frame: nothing has been laid out yet. A hidden panel keeps the
        // rect it had when it was last shown, which would leave a handle
        // floating over the canvas, so `preview_panel` clears its rect.
        if !rects.library.is_finite() || !rects.inspector.is_finite() {
            return;
        }

        let half = 3.5;
        let vstrip = |x: f32, y0: f32, y1: f32| {
            egui::Rect::from_min_max(egui::pos2(x - half, y0), egui::pos2(x + half, y1))
        };
        let hstrip = |y: f32, x0: f32, x1: f32| {
            egui::Rect::from_min_max(egui::pos2(x0, y - half), egui::pos2(x1, y + half))
        };

        // The side panels share the band between the toolbar and the preview,
        // so the vertical strips stop where the library was actually drawn.
        let band_top = rects.library.top();
        let band_bottom = rects.library.bottom();

        // -- left / right -------------------------------------------------
        let max_side = ctx.screen_rect().width() - model::PanelSizes::MIN_CANVAS;
        if let Some(x) = splitter_strip(
            ctx,
            "split-library",
            vstrip(rects.library.right(), band_top, band_bottom),
            true,
        ) {
            sizes.library = (x - rects.library.left())
                .clamp(*model::PanelSizes::LIBRARY.start(), *model::PanelSizes::LIBRARY.end())
                .min(max_side - sizes.inspector);
        }
        if let Some(x) = splitter_strip(
            ctx,
            "split-inspector",
            vstrip(rects.inspector.left(), band_top, band_bottom),
            true,
        ) {
            sizes.inspector = (rects.inspector.right() - x).clamp(
                *model::PanelSizes::INSPECTOR.start(),
                *model::PanelSizes::INSPECTOR.end(),
            ).min(max_side - sizes.library);
        }

        // -- bottom panels ------------------------------------------------
        if rects.tasks.is_finite() {
            if let Some(y) = splitter_strip(
                ctx,
                "split-tasks",
                hstrip(rects.tasks.top(), rects.tasks.left(), rects.tasks.right()),
                false,
            ) {
                let max_h = (rects.preview.top() - rects.library.top()).max(0.0)
                    + rects.preview.height()
                    - model::PanelSizes::MIN_CANVAS;
                sizes.tasks = (rects.tasks.bottom() - y)
                    .clamp(*model::PanelSizes::TASKS.start(), *model::PanelSizes::TASKS.end())
                    .min(max_h);
            }
        }
        if rects.preview.is_finite() {
            if let Some(y) = splitter_strip(
                ctx,
                "split-preview",
                hstrip(rects.preview.top(), rects.preview.left(), rects.preview.right()),
                false,
            ) {
                let max_h = rects.preview.bottom()
                    - rects.library.top()
                    - model::PanelSizes::MIN_CANVAS;
                sizes.preview = (rects.preview.bottom() - y)
                    .clamp(
                        *model::PanelSizes::PREVIEW.start(),
                        *model::PanelSizes::PREVIEW.end(),
                    )
                    .min(max_h);
            }
        }

        // -- file list inside the library panel ---------------------------
        //
        // The handle sits on the divider the file list was drawn with. It used
        // to be `preview_top - files`, which ignores the library frame's 8 px
        // inner margin and put the line 8 px below the edge it resizes.
        if rects.files.is_finite() {
            if let Some(y) = splitter_strip(
                ctx,
                "split-files",
                hstrip(rects.files.top(), rects.files.left(), rects.files.right()),
                false,
            ) {
                // The list lives inside the library, so it can only grow until
                // the node library is squeezed out — not until the *screen* is.
                let max_h = (rects.library_content.height() - LIBRARY_MIN_NODES)
                    .max(*model::PanelSizes::FILES.start());
                sizes.files = (rects.files.bottom() - y)
                    .clamp(*model::PanelSizes::FILES.start(), *model::PanelSizes::FILES.end())
                    .min(max_h);
            }
        }
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        let pal = theme::p();
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.bg))
            .show(ctx, |ui| {
                self.canvas_rect = ui.max_rect();
                if !self.view_fitted && self.fit_view(ctx) {
                    self.view_fitted = true;
                }
                // Clip everything the graph paints to the canvas. The library
                // already does this, but doing it here too means a port circle,
                // an edge or an overlay can never bleed onto a panel even if
                // the dependency changes its mind.
                ui.set_clip_rect(self.canvas_rect);
                paint_grid(
                    ui,
                    self.canvas_rect,
                    self.editor.pan_zoom.pan,
                    self.editor.pan_zoom.zoom,
                );
                let selected_before = self.editor.selected_nodes.clone();
                let response = self
                    .editor
                    .draw_graph_editor(ui, AllTemplates, &mut self.ctx, vec![]);
                // `egui_node_graph2` 0.7 opens its own node-finder popup on any
                // secondary release. The node library on the left already does
                // that job, so keep the popup closed (it is drawn before this
                // point next frame, hence the clear on every frame).
                self.editor.node_finder = None;
                // Rebuilt from egui's widget registry; see `node_rects`.
                let node_rects = node_rects(ctx, &self.editor);

                // `egui_node_graph2` clears `selected_nodes` on *any* click whose
                // pointer is outside the editor rect (`mouse.any_click() &&
                // !cursor_in_editor`), which includes the inspector — the one
                // place you go to edit the node you just selected. Without this
                // the panel blanked itself on the first click on any of its
                // controls. Restore the selection when the click was elsewhere
                // in the window; clicking empty canvas still deselects, because
                // that path goes through `click_on_background` with the pointer
                // inside the editor.
                if self.editor.selected_nodes.is_empty() && !response.cursor_in_editor {
                    self.editor.selected_nodes = selected_before
                        .iter()
                        .copied()
                        .filter(|n| self.editor.graph.nodes.get(*n).is_some())
                        .collect();
                }

            for r in response.node_responses {
                match r {
                    NodeResponse::User(FlowResponse::ProbeSource(json_id)) => {
                        if let Some(node_id) = graph::node_by_json_id(&self.editor.graph, json_id) {
                            if let NodeConfig::Source(cfg) = &self.editor.graph.nodes[node_id].user_data.config
                            {
                                if let Some(first) = source::expand(&cfg.paths, cfg.recursive, &cfg.extensions, cfg.skip_own_outputs)
                                    .first() {
                                    self.probes.remove(&first.to_string_lossy().to_string());
                                    self.ensure_probes();
                                }
                            }
                        }
                    }
                    NodeResponse::ConnectEventEnded {
                        output,
                        input,
                        input_hook,
                    } => {
                        let out_typ = self.editor.graph.get_output(output).typ;
                        let in_typ = self.editor.graph.get_input(input).typ;
                        self.editor.graph.remove_connection(input, output);
                        if out_typ != in_typ {
                            self.notify_err(format!(
                                "connection rejected: {} » {} (type mismatch)",
                                out_typ.name(),
                                in_typ.name()
                            ));
                        } else if graph::would_create_cycle(&self.editor.graph, output, input) {
                            self.notify_err("connection rejected: this would create a cycle");
                        } else {
                            self.editor.graph.add_connection(output, input, input_hook);
                        }
                    }
                    NodeResponse::DeleteNodeFull { node, .. } => {
                        self.node_deleted(
                            node.user_data.json_id,
                            node.user_data.kind() == NodeKind::Source,
                        );
                    }
                    _ => {}
                }
            }

            // Defence in depth: never keep a type-invalid edge around.
            for (input, output, why) in graph::invalid_connections(&self.editor.graph) {
                self.editor.graph.remove_connection(input, output);
                self.notify_err(format!("removed invalid connection: {}", why));
            }

            // Close-button backdrop.
            //
            // `egui_node_graph2` paints the ✕ as two bare strokes straight onto
            // the node body, which vanishes over a coloured title bar. A soft
            // disc behind it restores the affordance without patching the
            // dependency: we repaint disc + ✕ here, above the body.
            paint_close_buttons(
                ui,
                &self.editor.node_order,
                &node_rects,
                self.editor.pan_zoom.zoom,
                &pal,
            );

            // Selection ring. The library paints its own outline in
            // `Color32::WHITE.lighten(0.8)` behind the node, which is invisible on
            // the light palette, so draw one in the accent colour instead.
            //
            // The rects come from `node_rects`, which extends the widget rect
            // registered by the library to cover the card's rounded bottom cap.
            let zoom = self.editor.pan_zoom.zoom;
            for node in &self.editor.selected_nodes {
                if let Some(r) = node_rects.get(node) {
                    ui.painter().rect_stroke(
                        r.expand(1.5 * zoom),
                        egui::Rounding::same(6.0 * zoom),
                        egui::Stroke::new(1.5, pal.accent),
                    );
                }
            }

            // Drag a library item onto the canvas.
            if let Some(kind) = self.drag_kind {
                let pointer = ctx.pointer_latest_pos();
                if ctx.input(|i| i.pointer.any_released()) {
                    if let Some(p) = pointer {
                        if self.canvas_rect.contains(p) {
                            let pos = p - self.canvas_rect.min.to_vec2() - self.editor.pan_zoom.pan;
                            graph::add_node(&mut self.editor, &mut self.ctx, kind, pos);
                        }
                    }
                    self.drag_kind = None;
                } else if let Some(p) = pointer {
                    egui::Area::new(egui::Id::new("drag-ghost"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(p + egui::vec2(12.0, 12.0))
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .fill(pal.surface_hi)
                                .stroke(egui::Stroke::new(1.0, pal.accent))
                                .rounding(egui::Rounding::same(6.0))
                                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("+ {}", kind.title()))
                                            .size(theme::BODY)
                                            .color(pal.text),
                                    );
                                });
                        });
                }
            }

            // -- canvas status overlay (bottom-right) ----------------------
            let nodes = self.editor.graph.nodes.len();
            let zoom = self.editor.pan_zoom.zoom;
            // Anchored to the canvas corner, not to the screen: the chip is
            // sized by its text, so a fixed offset from the right edge left it
            // floating wherever the text happened to end.
            let screen = ctx.screen_rect();
            let inset = egui::vec2(
                screen.right() - self.canvas_rect.right() + 10.0,
                screen.bottom() - self.canvas_rect.bottom() + 10.0,
            );
            egui::Area::new(egui::Id::new("canvas-status"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_BOTTOM, -inset)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(pal.panel)
                        .rounding(egui::Rounding::same(6.0))
                        .stroke(egui::Stroke::new(1.0, pal.stroke))
                        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for t in [
                                    crate::graph::StreamType::File,
                                    crate::graph::StreamType::Video,
                                    crate::graph::StreamType::Audio,
                                ] {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(7.0, 7.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        rect.center(),
                                        3.5,
                                        theme::port_color(t),
                                    );
                                    ui.label(
                                        egui::RichText::new(t.name())
                                            .size(theme::CAPTION)
                                            .color(pal.dim),
                                    );
                                    ui.add_space(4.0);
                                }
                                ui.label(
                                    egui::RichText::new("·").size(theme::CAPTION).color(pal.faint),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{:.0}%", zoom * 100.0))
                                        .size(theme::CAPTION)
                                        .color(pal.faint),
                                );
                                ui.label(
                                    egui::RichText::new("·").size(theme::CAPTION).color(pal.faint),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{nodes} nodes"))
                                        .size(theme::CAPTION)
                                        .color(pal.faint),
                                );
                            });
                        });
                });

            // A hairline around the canvas. Panels and canvas are all the same
            // near-white in the light palette, so without it a card that ends
            // at the canvas edge looks like it is sliding under the inspector.
            ui.painter().rect_stroke(
                self.canvas_rect,
                egui::Rounding::ZERO,
                egui::Stroke::new(1.0, pal.stroke),
            );
        });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let mut open = self.show_help;
        egui::Window::new("fflow — how it works")
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(egui::RichText::new("Pipeline").strong());
                    ui.label(
                        "Source » Demux » (video / audio chains) » Package. Every file found by \
                         the Source node is compiled into its own ffmpeg command.",
                    );
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Ports").strong());
                    ui.label(
                        "Blue = File, orange = Video, green = Audio. Connections between different \
                         port types are refused immediately.",
                    );
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Shortcuts").strong());
                    ui.label("• drag empty canvas: box select");
                    ui.label("• Del / Backspace: delete the selected node · Esc: deselect");
                    ui.label("• middle mouse / cmd+drag: pan · wheel: zoom");
                    ui.label("• right click canvas: quick-add node menu");
                    ui.label("• drag a connection off its input port: disconnect");
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Templates").strong());
                    ui.label("{dir} {stem} {name} {ext} {index} {rel_dir}");
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Binaries").strong());
                    ui.label(format!(
                        "ffmpeg: {} · ffprobe: {}",
                        probe::ffmpeg_bin(),
                        probe::ffprobe_bin()
                    ));
                    ui.label("Override with FFLOW_FFMPEG / FFLOW_FFPROBE.");
                });
            });
        self.show_help = open;
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        self.toasts
            .retain(|t| t.born.elapsed() < Duration::from_secs(6));
        if self.toasts.is_empty() {
            return;
        }
        // Without this the last toast stays on screen until the next input
        // event, because an idle egui app does not repaint by itself.
        ctx.request_repaint_after(Duration::from_millis(200));
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(16.0, -16.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                for t in &self.toasts {
                    let color = if t.error {
                        egui::Color32::from_rgb(70, 32, 32)
                    } else {
                        egui::Color32::from_rgb(32, 46, 60)
                    };
                    egui::Frame::none()
                        .fill(color)
                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                        .rounding(4.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&t.msg).small()).wrap(),
                            );
                        });
                    ui.add_space(4.0);
                }
            });
    }
}

impl FflowApp {
    /// One frame: event pump + every panel. Split out of `eframe::App::update`
    /// so the headless test can drive it without an eframe `Frame`.
    pub fn draw_all(&mut self, ctx: &egui::Context) {
        self.poll_probes();
        self.poll_runner();
        self.handle_dropped(ctx);

        if let Some(at) = self.files_dirty_at {
            if at.elapsed() > Duration::from_millis(250) {
                self.refresh_files();
            }
        }
        self.ensure_probes();

        self.shortcuts(ctx);
        self.toolbar(ctx);
        self.clamp_panels(ctx);
        self.task_panel(ctx);
        self.preview_panel(ctx);
        self.library(ctx);
        self.inspector(ctx);
        self.canvas(ctx);
        self.splitters(ctx);
        self.help_window(ctx);
        self.toasts(ctx);

        if self.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }

    /// True while background work is outstanding (batch running, probes in
    /// flight, encoder detection). Drives repaint scheduling and lets the
    /// offscreen renderer wait for a representative frame.
    pub fn is_busy(&self) -> bool {
        self.runner.is_running()
            || !self.probe_inflight.is_empty()
            || crate::encoders::is_detecting()
    }
}

impl eframe::App for FflowApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw_all(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        save_settings(&self.settings);
        if let Ok(json) = graph::to_workflow(&self.editor).to_json() {
            let _ = std::fs::write(config_dir().join("autosave.fflow.json"), json);
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Stable ids for the cards in the library and the file list. They are handed
/// to `theme::card` so the same widget can be found again by id (tests, and the
/// drag ghost that follows the pointer).
fn library_card_id(kind: NodeKind) -> egui::Id {
    egui::Id::new(("lib-card", kind.id()))
}

/// Soft disc + ✕ on top of each node, replacing the bare strokes the
/// dependency draws.
///
/// The geometry mirrors `egui_node_graph2`'s `close_button` exactly (margin
/// 8 px, glyph 10 px, both scaled by zoom), so the repainted cross lands on the
/// original and the hit area still belongs to the dependency. The card rects
/// come from `node_rects`.
fn paint_close_buttons(
    ui: &egui::Ui,
    order: &[egui_node_graph2::NodeId],
    rects: &std::collections::HashMap<egui_node_graph2::NodeId, egui::Rect>,
    zoom: f32,
    pal: &theme::Palette,
) {
    let painter = ui.painter();
    for (i, &node) in order.iter().enumerate() {
        let Some(card) = rects.get(&node) else {
            continue;
        };
        let size = 10.0 * zoom;
        let offs = 8.0 * zoom + size / 2.0;
        let rect = egui::Rect::from_center_size(
            egui::pos2(card.right() - offs, card.top() + offs),
            egui::vec2(size, size),
        );
        // A slightly larger disc than the glyph, so it reads as a button.
        let disc = rect.expand(5.0 * zoom);
        // Every ✕ is painted after every card, so a card sitting under a
        // neighbour would otherwise stamp its ✕ on top of that neighbour's
        // body. Skip any ✕ whose centre is covered by a card drawn later.
        let covered = order[i + 1..].iter().any(|later| {
            rects
                .get(later)
                .map(|r| r.contains(disc.center()))
                .unwrap_or(false)
        });
        if covered {
            continue;
        }
        // Clip to the card as well: the disc is a little wider than the glyph
        // and must never spill past the card it belongs to.
        let painter = painter.with_clip_rect(disc.intersect(*card));
        let hot = ui.rect_contains_pointer(disc);
        let dark = pal.bg.r() < 0x80;
        let fill = match (dark, hot) {
            (true, false) => egui::Color32::from_black_alpha(70),
            (true, true) => egui::Color32::from_black_alpha(130),
            (false, false) => egui::Color32::from_white_alpha(120),
            (false, true) => egui::Color32::from_white_alpha(220),
        };
        painter.rect_filled(disc, egui::Rounding::same(disc.height() * 0.5), fill);
        let color = if hot { pal.text } else { pal.dim };
        let stroke = egui::Stroke::new(1.6, color);
        painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
        painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
    }
}

/// Painted rect of every node card, keyed by node id.
///
/// `egui_node_graph2` 0.7 computes this rect while drawing each card but only
/// keeps it in a private field, so callers cannot read it. It does register the
/// card's interaction rect in egui's widget registry under `(node, "window")`,
/// and that interaction rect *is* the card's outer rect (`child_ui.min_rect()`
/// expanded by the card margin). We read it back here and add the rounded
/// bottom cap that the painted card extends beyond it.
///
/// The interaction rect comes from the previous pass, so a node that is being
/// dragged or zoomed is one frame behind. That was already true of the hit rect
/// the app used before, and it only shows while the view is moving.
fn node_rects(
    ctx: &egui::Context,
    editor: &GraphEditor,
) -> HashMap<egui_node_graph2::NodeId, egui::Rect> {
    let zoom = editor.pan_zoom.zoom;
    let cap = bottom_cap_height(ctx, zoom);
    editor
        .node_order
        .iter()
        .filter_map(|&node| {
            let mut rect = ctx.read_response(egui::Id::new((node, "window")))?.rect;
            rect.max.y += cap;
            Some((node, rect))
        })
        .collect()
}

/// How far the painted card extends below the interaction rect.
///
/// Upstream builds the card from `titlebar_rect ∪ body_rect ∪ bottom_body_rect`.
/// The latter overshoots the outer rect by `titlebar_height / 2 - rounding`,
/// where `titlebar_height = title_height + margin.y`, and `title_height` itself
/// is the title row plus `item_spacing.y` plus `margin.y` (the crate calls
/// `add_space(margin.y)` before measuring it). With the app's 6 px item spacing
/// and upstream's 5 px margin and 4 px rounding that reduces to
/// `title_row / 2 + 4 * zoom`.
fn bottom_cap_height(ctx: &egui::Context, zoom: f32) -> f32 {
    let style = ctx.style();
    let font = egui::TextStyle::Button.resolve(&style);
    let row = ctx.fonts(|f| f.row_height(&egui::FontId::new(font.size * zoom, font.family)));
    row * 0.5 + 4.0 * zoom
}

/// Foldable group title for the library: caret, name, count.
/// egui's `CollapsingHeader` brings its own frame and padding, which fights the
/// flat shadcn-ish panel, so this is painted by hand.
fn category_header(ui: &mut egui::Ui, title: &str, count: usize, open: bool) -> egui::Response {
    let pal = theme::p();
    let h = 22.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let hot = resp.hovered();
        let caret = if open { "▾" } else { "▸" };
        ui.painter().text(
            rect.left_center() + egui::vec2(2.0, 0.0),
            egui::Align2::LEFT_CENTER,
            caret,
            egui::FontId::proportional(11.0),
            if hot { pal.text } else { pal.faint },
        );
        ui.painter().text(
            rect.left_center() + egui::vec2(16.0, 0.0),
            egui::Align2::LEFT_CENTER,
            title.to_uppercase(),
            egui::FontId::proportional(theme::CAPTION),
            if hot { pal.text } else { pal.faint },
        );
        ui.painter().text(
            rect.right_center() - egui::vec2(2.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(theme::CAPTION),
            pal.faint,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn file_card_id(index: usize) -> egui::Id {
    egui::Id::new(("file-card", index))
}

fn icon_for(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Source => "📁",
        NodeKind::Demux => "🔀",
        NodeKind::Scale => "🖼",
        NodeKind::Pad => "▣",
        NodeKind::Crop => "✂",
        NodeKind::Rotate => "🔄",
        NodeKind::Fps => "⏱",
        NodeKind::Color => "🎨",
        NodeKind::Lut => "🌈",
        NodeKind::Sharpen => "🔍",
        NodeKind::Denoise => "☁",
        NodeKind::Overlay => "📌",
        NodeKind::Volume => "🔉",
        NodeKind::Loudness => "📈",
        NodeKind::Band => "📉",
        NodeKind::Tempo => "⏩",
        NodeKind::Channels => "🎧",
        NodeKind::Resample => "📊",
        NodeKind::RawFilter => "✏",
        NodeKind::Condition => "⚖",
        NodeKind::VideoEncoder => "🎥",
        NodeKind::AudioEncoder => "🔊",
        NodeKind::Package => "📦",
    }
}

fn workflow_root(editor: &GraphEditor) -> Option<PathBuf> {
    for node_id in graph::source_nodes(&editor.graph) {
        if let NodeConfig::Source(cfg) = &editor.graph.nodes[node_id].user_data.config {
            for p in &cfg.paths {
                let path = PathBuf::from(p);
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
    }
    None
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let dir = base.join("fflow");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load_settings() -> Settings {
    std::fs::read_to_string(config_dir().join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(config_dir().join("settings.json"), json);
    }
}

fn load_autosave() -> Option<Workflow> {
    let text = std::fs::read_to_string(config_dir().join("autosave.fflow.json")).ok()?;
    Workflow::from_json(&text).ok()
}

fn check_tools() -> Option<String> {
    let out = std::process::Command::new(probe::ffmpeg_bin())
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match out {
        Ok(s) if s.success() => None,
        _ => Some(format!(
            "⚠ ffmpeg not found ({}); install it or set FFLOW_FFMPEG",
            probe::ffmpeg_bin()
        )),
    }
}

/// Dot grid behind the node graph. Decorative: it only needs to move with the
/// canvas and scale with the zoom, not to align to graph coordinates.
fn paint_grid(ui: &egui::Ui, rect: egui::Rect, pan: egui::Vec2, zoom: f32) {
    let pal = theme::p();
    let step = 26.0 * zoom;
    if step < 8.0 {
        return;
    }
    let dot = egui::Color32::from_rgba_unmultiplied(
        pal.stroke.r(),
        pal.stroke.g(),
        pal.stroke.b(),
        110,
    );
    let painter = ui.painter();
    let ox = pan.x.rem_euclid(step);
    let oy = pan.y.rem_euclid(step);
    let mut y = rect.top() + oy;
    while y < rect.bottom() {
        let mut x = rect.left() + ox;
        while x < rect.right() {
            painter.circle_filled(egui::pos2(x, y), 0.9, dot);
            x += step;
        }
        y += step;
    }
}

/// Colour one argv token in the command preview.
fn command_token_color(arg: &str, prev: Option<&str>, is_last: bool) -> egui::Color32 {
    let pal = theme::p();
    if is_last {
        return pal.ok; // output path
    }
    if arg.starts_with('-') {
        return pal.accent; // option name
    }
    if matches!(
        prev,
        Some("-c:v" | "-c:a" | "-c:s" | "-vcodec" | "-acodec" | "-preset" | "-pix_fmt" | "-f" | "-rc")
    ) {
        return pal.video; // codec / container / encoder option value
    }
    if arg.starts_with('[') || arg.contains("]=") {
        return theme::category_color("Filters"); // filter graph expression
    }
    if arg.contains('/') || arg.contains('\\') {
        return pal.file; // a path
    }
    pal.dim
}

/// Draw one splitter strip and report the pointer position along its axis while
/// it is being dragged.
///
/// Lives in its own `Area` layer created after the canvas, which is what makes it
/// win the pointer against the canvas's own click+drag background.
fn splitter_strip(
    ctx: &egui::Context,
    id: &str,
    rect: egui::Rect,
    vertical: bool,
) -> Option<f32> {
    let pal = theme::p();
    let resp = egui::Area::new(egui::Id::new(("fflow-splitter", id)))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(rect.size(), egui::Sense::drag());
            // A permanent hairline marks the panel boundary; the accent takes
            // over only while the handle is live.
            let c = response.rect.center();
            let (stroke, width) = if response.dragged() {
                (pal.accent, 2.0)
            } else if response.hovered() {
                (pal.accent, 1.0)
            } else {
                (pal.stroke, 1.0)
            };
            let stroke = egui::Stroke::new(width, stroke);
            if vertical {
                painter.vline(c.x, response.rect.y_range(), stroke);
            } else {
                painter.hline(response.rect.x_range(), c.y, stroke);
            }
            response
        })
        .inner;

    if resp.hovered() || resp.dragged_by(egui::PointerButton::Primary) {
        ctx.set_cursor_icon(if vertical {
            egui::CursorIcon::ResizeHorizontal
        } else {
            egui::CursorIcon::ResizeVertical
        });
    }
    // Primary only: a secondary (right) drag anywhere near a strip used to
    // resize the panel, which then persisted and moved the divider a little
    // further every session.
    if resp.dragged_by(egui::PointerButton::Primary) {
        return resp.interact_pointer_pos().map(|p| if vertical { p.x } else { p.y });
    }
    None
}

/// Vertical hairline used to group toolbar actions.
fn toolbar_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 18.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.top()..=rect.bottom(),
        egui::Stroke::new(1.0, theme::p().stroke),
    );
}

fn reveal_in_explorer(dir: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(dir).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

pub(crate) fn install_fonts(ctx: &egui::Context) {
    // egui's built-in fonts have no CJK glyphs; borrow one from the system so
    // that non-ASCII file names are readable.
    let candidates = [
        "C:/Windows/Fonts/simhei.ttf",
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/simsun.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_input() -> egui::RawInput {
        let mut input = egui::RawInput::default();
        input.screen_rect =
            Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0)));
        input
    }

    /// Minimal headless driver: owns the context and a monotonically increasing
    /// frame clock so egui's click/drag detection behaves like a real session.
    struct Harness {
        ctx: egui::Context,
        app: FflowApp,
        time: f64,
        size: egui::Vec2,
    }

    impl Harness {
        fn new() -> Self {
            Self::sized(1440.0, 900.0)
        }

        fn sized(width: f32, height: f32) -> Self {
            let ctx = egui::Context::default();
            let mut app = FflowApp::build(&ctx, false);
            // The real settings file may hold whatever the user last dragged;
            // the layout tests need a known starting point.
            app.settings.panels = model::PanelSizes::default();
            let mut h = Self {
                ctx,
                app,
                time: 0.0,
                size: egui::vec2(1440.0, 900.0),
            };
            h.size = egui::vec2(width, height);
            // Settle frames: panel sizes, fonts and the per-node interaction
            // rects the library caches in egui memory all need a few passes.
            for _ in 0..6 {
                h.frame(Vec::new());
            }
            h
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 1.0 / 60.0;
            let mut input = raw_input();
            input.time = Some(self.time);
            input.events = events;
            input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size));
            let app = &mut self.app;
            let _ = self.ctx.run(input, |c| app.draw_all(c));
        }

        /// Screen rect of a node, as the library registered it for hit-testing.
        fn node_rect(&self, node: egui_node_graph2::NodeId) -> egui::Rect {
            self.ctx
                .read_response(egui::Id::new((node, "window")))
                .unwrap_or_else(|| panic!("node {:?} has no window response", node))
                .rect
        }

        fn press(&mut self, pos: egui::Pos2) {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ]);
        }

        fn release(&mut self, pos: egui::Pos2) {
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }]);
        }

        fn click(&mut self, pos: egui::Pos2) {
            self.press(pos);
            self.release(pos);
        }

        fn move_to(&mut self, pos: egui::Pos2) {
            self.frame(vec![egui::Event::PointerMoved(pos)]);
        }

        /// Screen rect of a splitter strip, as the `Area` registered it.
        fn strip_rect(&self, name: &str) -> egui::Rect {
            let layer = egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new(("fflow-splitter", name)),
            );
            self.ctx
                .read_response(layer.id.with("move"))
                .unwrap_or_else(|| panic!("splitter {name} has no response"))
                .rect
        }

        fn key(&mut self, key: egui::Key) {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }]);
        }

        fn wheel(&mut self, pos: egui::Pos2, dy: f32) {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, dy),
                    modifiers: egui::Modifiers::default(),
                },
            ]);
        }

        /// Press, move and release with the middle mouse button (canvas pan).
        fn middle_drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
            let button = egui::PointerButton::Middle;
            self.frame(vec![
                egui::Event::PointerMoved(from),
                egui::Event::PointerButton {
                    pos: from,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ]);
            for i in 1..=4 {
                let p = from + (to - from) * (i as f32 / 4.0);
                self.frame(vec![egui::Event::PointerMoved(p)]);
            }
            self.frame(vec![egui::Event::PointerButton {
                pos: to,
                button,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }]);
        }
    }

    /// Drives a full frame (all panels) headlessly for every node selection,
    /// which exercises every inspector form as well.
    #[test]
    fn whole_ui_renders_headless() {
        let egui_ctx = egui::Context::default();
        let mut app = FflowApp::build(&egui_ctx, false);
        let input = raw_input();

        let nodes: Vec<_> = app.editor.node_order.clone();
        let expected = crate::graph::default_workflow(None).nodes.len();
        assert_eq!(nodes.len(), expected, "default workflow should have {expected} nodes");

        for node in nodes {
            app.editor.selected_nodes = vec![node];
            for _ in 0..2 {
                let _ = egui_ctx.run(input.clone(), |c| app.draw_all(c));
            }
        }

        app.editor.selected_nodes.clear();
        app.show_help = true;
        app.notify("hello");
        for _ in 0..2 {
            let _ = egui_ctx.run(input.clone(), |c| app.draw_all(c));
        }
    }

    #[test]
    fn compile_reports_missing_package() {
        let egui_ctx = egui::Context::default();
        let mut app = FflowApp::build(&egui_ctx, false);
        // Remove the Package node: the preview must explain the problem
        // instead of panicking.
        let pkg = graph::package_nodes(&app.editor.graph)[0];
        app.editor.graph.remove_node(pkg);
        app.editor.node_order.retain(|n| *n != pkg);
        assert!(app.compile_current().is_err());
    }

    /// Dragging a node's titlebar must move it, and must not be swallowed by
    /// the canvas grid / overlay layers drawn on top of the editor.
    #[test]
    fn dragging_a_node_moves_it() {
        let mut h = Harness::new();
        let node = h.app.editor.node_order[0];
        let before = h.app.editor.node_positions[node];
        let rect = h.node_rect(node);

        // Grab the titlebar, well clear of the close button on the right.
        let grab = rect.left_top() + egui::vec2(30.0, 9.0);
        h.press(grab);
        // Several intermediate moves: egui needs a couple of frames past the
        // drag threshold before it reports a delta.
        for i in 1..=4 {
            h.move_to(grab + egui::vec2(10.0 * i as f32, 7.0 * i as f32));
        }
        h.release(grab + egui::vec2(40.0, 28.0));
        h.frame(Vec::new());

        let after = h.app.editor.node_positions[node];
        let moved = after - before;
        assert!(
            moved.length() > 20.0,
            "node did not move: before={:?} after={:?}",
            before,
            after
        );
        assert!(
            (moved.x - 40.0).abs() < 4.0 && (moved.y - 28.0).abs() < 4.0,
            "node moved by {:?}, expected about (40, 28)",
            moved
        );
    }

    /// Clicking a node's close button must remove it cleanly — no panic, and
    /// no stale ids left in the editor state.
    ///
    /// This goes through the real `DeleteNodeUi` path in `egui_node_graph2`,
    /// which is what used to panic one frame later with "invalid SlotMap key
    /// used" whenever the deleted node had a connected input.
    #[test]
    fn close_button_deletes_every_node_cleanly() {
        for index in 0..5 {
            let mut h = Harness::new();
            let node = h.app.editor.node_order[index];
            let rect = h.node_rect(node);
            let nodes_before = h.app.editor.graph.nodes.len();

            // The library draws the close button inset by margin + size/2 = 13px.
            let hit = rect.right_top() + egui::vec2(-13.0, 13.0);
            h.click(hit);
            h.frame(Vec::new());

            assert_eq!(
                h.app.editor.graph.nodes.len(),
                nodes_before - 1,
                "close button click did not delete node {index}"
            );
            assert!(!h.app.editor.node_order.contains(&node));
            assert!(h.app.editor.node_positions.get(node).is_none());
            assert!(!h.app.editor.selected_nodes.contains(&node));
            // Every surviving connection must still resolve.
            for (input, output) in h.app.editor.graph.iter_connections() {
                assert!(h.app.editor.graph.try_get_input(input).is_some());
                assert!(h.app.editor.graph.try_get_output(output).is_some());
            }
            // ...and the app keeps rendering + compiling.
            for _ in 0..2 {
                h.frame(Vec::new());
            }
            let _ = h.app.compile_current();
        }
    }

    /// Clicking a node's titlebar must select it. This shares the same
    /// interaction rect as dragging, so it guards the fix from the other side:
    /// if a child widget ever starts swallowing titlebar clicks again, both
    /// this and `dragging_a_node_moves_it` fail.
    #[test]
    fn clicking_a_node_titlebar_selects_it() {
        let mut h = Harness::new();
        h.app.editor.selected_nodes.clear();
        h.frame(Vec::new());
        assert!(h.app.editor.selected_nodes.is_empty());

        let node = h.app.editor.node_order[2];
        let rect = h.node_rect(node);
        h.click(rect.left_top() + egui::vec2(30.0, 9.0));
        h.frame(Vec::new());

        assert_eq!(h.app.editor.selected_nodes, vec![node]);
    }

    /// Deleting each node in turn must leave the app in a renderable, compilable
    /// state — the inspector, preview and task panel all run on the same frame.
    #[test]
    fn deleting_any_node_is_safe() {
        for index in 0..5 {
            let mut h = Harness::new();
            let node = h.app.editor.node_order[index];

            // Mirror what the library does on `DeleteNodeUi`.
            let (removed, _) = h.app.editor.graph.remove_node(node);
            h.app.editor.node_positions.remove(node);
            h.app.editor.node_order.retain(|n| *n != node);
            h.app.editor.selected_nodes.retain(|n| *n != node);
            h.app
                .node_deleted(removed.user_data.json_id, removed.user_data.kind() == NodeKind::Source);
            for _ in 0..3 {
                h.frame(Vec::new());
            }
            let _ = h.app.compile_current();
        }
    }

    /// Clicking a control in the inspector must not drop the node selection.
    ///
    /// `egui_node_graph2` clears `selected_nodes` on *any* click whose pointer
    /// is outside the editor rect (`mouse.any_click() && !cursor_in_editor`),
    /// and the inspector is by definition outside it. That made the panel blank
    /// itself the moment you touched one of its controls.
    #[test]
    fn clicking_the_inspector_keeps_the_selection() {
        let mut h = Harness::new();
        let node = h.app.editor.node_order[2];
        h.click(h.node_rect(node).left_top() + egui::vec2(30.0, 9.0));
        h.frame(Vec::new());
        assert_eq!(h.app.editor.selected_nodes, vec![node]);

        let screen = h.ctx.screen_rect();
        let spot = egui::pos2(
            screen.right() - h.app.settings.panels.inspector * 0.5,
            screen.bottom() - 24.0,
        );
        h.click(spot);
        h.frame(Vec::new());
        assert_eq!(
            h.app.editor.selected_nodes,
            vec![node],
            "clicking inside the inspector dropped the selection"
        );

        // Clicking empty canvas must still deselect. The default nodes sit in
        // the top-left of the canvas, so aim at the empty strip along the bottom.
        h.click(h.app.canvas_rect.left_bottom() + egui::vec2(40.0, -40.0));
        h.frame(Vec::new());
        assert!(h.app.editor.selected_nodes.is_empty());
    }

    /// Every splitter strip must actually resize its panel.
    #[test]
    fn splitters_resize_every_panel() {
        // Drag each strip away from its panel: +x for the library, -x for the
        // inspector (anchored right), and -y for the three bottom strips.
        let cases = [
            ("split-library", 60.0, 0.0),
            ("split-inspector", -60.0, 0.0),
            ("split-tasks", 0.0, -49.0),
            ("split-preview", 0.0, -40.0),
            ("split-files", 0.0, -40.0),
        ];
        for (name, dx, dy) in cases {
            let mut h = Harness::new();
            let before = h.app.settings.panels;
            let canvas_before = h.app.canvas_rect;
            let strip = h.strip_rect(name);
            assert!(strip.width() > 0.0 && strip.height() > 0.0, "{name} has no area");

            let grab = strip.center();
            h.press(grab);
            for i in 1..=4 {
                h.move_to(grab + egui::vec2(dx, dy) * (i as f32 / 4.0));
            }
            h.release(grab + egui::vec2(dx, dy));
            h.frame(Vec::new());

            let after = h.app.settings.panels;
            let changed = match name {
                "split-library" => after.library - before.library,
                "split-inspector" => after.inspector - before.inspector,
                "split-tasks" => after.tasks - before.tasks,
                "split-preview" => after.preview - before.preview,
                _ => after.files - before.files,
            };
            assert!(
                changed.abs() > 20.0,
                "{name}: panel size barely moved ({before:?} -> {after:?})"
            );
            assert!(
                changed > 0.0,
                "{name}: dragging away from the panel should grow it ({before:?} -> {after:?})"
            );
            // The canvas must give up exactly the space the side panels took.
            if dx != 0.0 {
                let grew = h.app.canvas_rect.width() - canvas_before.width();
                assert!(
                    (grew + changed).abs() < 1.5,
                    "{name}: canvas width moved by {grew}, panel by {changed}"
                );
            }
        }
    }

    /// Each handle must sit on the edge it resizes.
    ///
    /// The file-list handle used to be placed at `preview_top - files`, which
    /// ignores the 8 px inner margin of the library frame the list is nested
    /// in — so the line and the hit area hung 8 px below the divider they were
    /// supposed to move.
    #[test]
    fn splitter_strips_sit_on_the_panel_edges() {
        let h = Harness::new();
        let r = h.app.panel_rects;
        let near = |a: f32, b: f32, what: &str| {
            assert!(
                (a - b).abs() <= 1.0,
                "{what}: handle at {a:.1}, edge at {b:.1}"
            );
        };
        near(h.strip_rect("split-library").center().x, r.library.right(), "library");
        near(h.strip_rect("split-inspector").center().x, r.inspector.left(), "inspector");
        near(h.strip_rect("split-tasks").center().y, r.tasks.top(), "tasks");
        near(h.strip_rect("split-preview").center().y, r.preview.top(), "preview");
        near(h.strip_rect("split-files").center().y, r.files.top(), "files");
    }

    /// The file list may grow, but never over the node library it lives with.
    #[test]
    fn file_list_cannot_swallow_the_node_library() {
        let mut h = Harness::new();
        let grab = h.strip_rect("split-files").center();
        h.press(grab);
        for i in 1..=6 {
            h.move_to(grab - egui::vec2(0.0, 80.0) * (i as f32 / 6.0));
        }
        h.release(grab - egui::vec2(0.0, 80.0));
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        let files = h.app.panel_rects.files;
        let lib = h.app.panel_rects.library;
        assert!(
            files.top() >= lib.top() + LIBRARY_MIN_NODES - 1.0,
            "file list starts at {:.0}, library at {:.0}: the node cards are covered",
            files.top(),
            lib.top()
        );
        // And the clamp in `clamp_panels` agrees, so a reload keeps it sane.
        assert!(h.app.settings.panels.files <= h.app.panel_rects.library_content.height());
    }

    /// On a cramped window the graph is fitted once, so no card ends up behind
    /// a panel. 1080x700 with both side panels leaves ~460 px of canvas — the
    /// old 100% view put the Video Encoder card under the inspector.
    #[test]
    fn startup_fits_the_whole_graph_into_the_canvas() {
        let h = Harness::sized(1080.0, 700.0);
        let canvas = h.app.canvas_rect;
        assert!(
            h.app.editor.pan_zoom.zoom < 0.95,
            "a 1080x700 window should have zoomed out to fit, got {:.2}",
            h.app.editor.pan_zoom.zoom
        );
        let mut bbox = egui::Rect::NOTHING;
        for node in &h.app.editor.node_order {
            bbox = bbox.union(h.node_rect(*node));
        }
        assert!(
            canvas.contains_rect(bbox),
            "graph {bbox:?} does not fit the canvas {canvas:?}"
        );
    }

    /// Cards are diagrams, not forms: they must stay small enough that five of
    /// them fit across a normal canvas.
    ///
    /// Measured through the interaction rect (one frame stale, hence the slack),
    /// with the unpatched crates.io crate: Source 154x99, Demux 154x122,
    /// encoders 154x115, Package 154x151 — the Package is the tall one because
    /// both of its inputs are fan-in ports, and the library reserves a second
    /// hook row on those. (With the `egui_node_graph2` fork of 12/4 px card
    /// margins and 3 px body spacing the same cards were 148 wide and 87–136
    /// tall; see *Known visual limitations* in the README.)
    #[test]
    fn cards_stay_compact() {
        let h = Harness::new();
        assert!((h.app.editor.pan_zoom.zoom - 1.0).abs() < 0.05);
        for node in &h.app.editor.node_order {
            let r = h.node_rect(*node);
            assert!(
                r.width() <= 170.0,
                "{} is {:.0} px wide",
                h.app.editor.graph[*node].label,
                r.width()
            );
            assert!(
                r.height() <= 165.0,
                "{} is {:.0} px tall",
                h.app.editor.graph[*node].label,
                r.height()
            );
        }
    }

    /// Middle-drag on empty canvas pans the view.
    #[test]
    fn canvas_pans_with_middle_drag() {
        let mut h = Harness::new();
        let before = h.app.editor.pan_zoom.pan;
        let from = h.app.canvas_rect.left_bottom() + egui::vec2(40.0, -40.0);
        h.middle_drag(from, from + egui::vec2(80.0, -50.0));
        h.frame(Vec::new());

        let moved = h.app.editor.pan_zoom.pan - before;
        assert!(
            (moved.x - 80.0).abs() < 4.0 && (moved.y + 50.0).abs() < 4.0,
            "pan moved by {moved:?}, expected (80, -50)"
        );
    }

    /// Zooming must not change a card's *width* relative to the graph.
    ///
    /// A fixed 146 px width floor in `right_label` used to inflate cards to
    /// 247 graph px at 0.68x and shrink them to 127 at 1.46x; the floor is now
    /// zoom-scaled, and this test locks that in.
    ///
    /// Height does **not** hold still, because `egui_node_graph2` compares an
    /// unscaled `port_height` (10 or 25 px) against a scaled row height and
    /// adds the difference as padding: 8–28 px per inline input depending on
    /// the zoom. With the crate unpatched the Package card measures 126 graph
    /// px tall at 1.56x and 245 at 0.40x (0.84x–1.62x of its size at 1.0). The
    /// bound below only catches a runaway layout, not the upstream drift; see
    /// *Known visual limitations* in the README for why this is not fixed.
    ///
    /// Measured through the interaction rect, which lags a frame, hence the
    /// tolerance.
    #[test]
    fn zoom_keeps_card_width_stable() {
        let mut h = Harness::new();
        h.frame(Vec::new());
        // The Package card is the worst case: two fan-in inputs, which is where
        // the dependency's unscaled padding hurts most.
        let node = h.app.editor.node_order[4];
        let reference = h.node_rect(node);

        let down = [-60.0; 8];
        let up = [60.0; 12];
        for step in down.into_iter().chain(up) {
            h.wheel(h.app.canvas_rect.center(), step);
            for _ in 0..3 {
                h.frame(Vec::new());
            }
            let zoom = h.app.editor.pan_zoom.zoom;
            let rect = h.node_rect(node);
            let (w, ht) = (rect.width() / zoom, rect.height() / zoom);
            assert!(
                (w - reference.width()).abs() < reference.width() * 0.08,
                "card is {w:.0} graph px wide at zoom {zoom:.2}, expected {:.0}",
                reference.width()
            );
            assert!(
                ht > reference.height() * 0.75 && ht < reference.height() * 1.8,
                "card is {ht:.0} graph px tall at zoom {zoom:.2}, expected roughly {:.0}",
                reference.height()
            );
        }
    }

    /// The wheel zooms the canvas, and zooming keeps the graph coherent.
    #[test]
    fn canvas_zooms_with_the_wheel() {
        let mut h = Harness::new();
        let before = h.app.editor.pan_zoom.zoom;
        h.wheel(h.app.canvas_rect.center(), 60.0);
        h.frame(Vec::new());
        let zoomed_in = h.app.editor.pan_zoom.zoom;
        assert!(zoomed_in > before, "wheel up did not zoom in: {before} -> {zoomed_in}");

        h.wheel(h.app.canvas_rect.center(), -60.0);
        h.frame(Vec::new());
        let zoomed_out = h.app.editor.pan_zoom.zoom;
        assert!(zoomed_out < zoomed_in, "wheel down did not zoom out");
        // Zooming rescales every stored node position, so the graph must still
        // be renderable and compilable afterwards.
        for _ in 0..2 {
            h.frame(Vec::new());
        }
        let _ = h.app.compile_current();
    }

    /// Dragging a box over the canvas selects the nodes inside it.
    #[test]
    fn box_selection_selects_the_nodes_inside() {
        let mut h = Harness::new();
        h.app.editor.selected_nodes.clear();
        h.frame(Vec::new());

        // Pin two nodes together and push the rest far away, so the box
        // covers exactly two regardless of the default layout.
        let first = h.app.editor.node_order[0];
        let second = h.app.editor.node_order[1];
        h.app.editor.node_positions.insert(first, egui::pos2(60.0, 60.0));
        h.app.editor.node_positions.insert(second, egui::pos2(300.0, 60.0));
        for (i, n) in h.app.editor.node_order.iter().enumerate().skip(2) {
            h.app.editor.node_positions.insert(*n, egui::pos2(60.0 + i as f32 * 240.0, 420.0));
        }
        h.frame(Vec::new());

        let a = h.node_rect(first);
        let b = h.node_rect(second);
        let start = a.left_top().min(b.left_top()) - egui::vec2(12.0, 12.0);
        let end = a.right_bottom().max(b.right_bottom()) + egui::vec2(12.0, 12.0);

        h.press(start);
        for i in 1..=4 {
            h.move_to(start + (end - start) * (i as f32 / 4.0));
        }
        h.release(end);
        h.frame(Vec::new());

        let mut got = h.app.editor.selected_nodes.clone();
        got.sort();
        let mut want = vec![first, second];
        want.sort();
        assert_eq!(
            got, want,
            "box selection did not pick up the two nodes it covered"
        );
    }

    /// Clicking a library card adds that node at the centre of the canvas.
    #[test]
    fn clicking_a_library_card_adds_a_node() {
        let mut h = Harness::new();
        let before = h.app.editor.graph.nodes.len();
        let card = h
            .ctx
            .read_response(library_card_id(NodeKind::Scale))
            .expect("library card was not drawn")
            .rect;
        h.click(card.center());
        h.frame(Vec::new());

        assert_eq!(h.app.editor.graph.nodes.len(), before + 1);
        let new = *h.app.editor.selected_nodes.first().expect("new node not selected");
        assert_eq!(
            h.app.editor.graph.nodes[new].user_data.kind(),
            NodeKind::Scale
        );
    }

    /// Dragging a library card onto the canvas drops the node where the pointer
    /// is released.
    #[test]
    fn dragging_a_library_card_drops_a_node_on_the_canvas() {
        let mut h = Harness::new();
        let before = h.app.editor.graph.nodes.len();
        let card = h
            .ctx
            .read_response(library_card_id(NodeKind::Scale))
            .expect("library card was not drawn")
            .rect;
        let drop_at = h.app.canvas_rect.center() + egui::vec2(140.0, 120.0);

        h.press(card.center());
        for i in 1..=4 {
            h.move_to(card.center() + (drop_at - card.center()) * (i as f32 / 4.0));
        }
        h.release(drop_at);
        h.frame(Vec::new());

        assert_eq!(h.app.editor.graph.nodes.len(), before + 1);
        let new = *h.app.editor.selected_nodes.first().expect("new node not selected");
        let pos = h.app.editor.node_positions[new];
        let expected =
            drop_at - h.app.canvas_rect.min.to_vec2() - h.app.editor.pan_zoom.pan;
        assert!(
            (pos - expected).length() < 2.0,
            "node landed at {pos:?}, expected {expected:?}"
        );
    }

    /// Clicking a file card moves the preview to that file.
    #[test]
    fn clicking_a_file_card_switches_the_preview() {
        let mut h = Harness::new();
        h.app.files = vec![PathBuf::from("Z:/a.mp4"), PathBuf::from("Z:/b.mp4")];
        // Pretend both were already rejected so the test never spawns ffprobe.
        h.app.probe_failed.insert("Z:/a.mp4".into());
        h.app.probe_failed.insert("Z:/b.mp4".into());
        h.frame(Vec::new());

        let card = h
            .ctx
            .read_response(file_card_id(1))
            .expect("file card was not drawn")
            .rect;
        h.click(card.center());
        h.frame(Vec::new());
        assert_eq!(h.app.preview_index, 1);
    }

    /// A file ffprobe cannot open must not be re-probed on every frame, and
    /// must not block the batch behind "still probing".
    #[test]
    fn unreadable_files_are_not_probed_forever() {
        let mut h = Harness::new();
        let bad = PathBuf::from("Z:/does/not/exist.mp4");
        h.app.files = vec![bad.clone()];
        h.app.probe_failed.insert(bad.to_string_lossy().to_string());
        h.app.ensure_probes();
        assert!(
            h.app.probe_inflight.is_empty(),
            "a file already rejected by ffprobe was probed again"
        );

        // The preview says why instead of pretending to still be probing.
        let err = h.app.compile_current().unwrap_err();
        assert!(err.contains("could not be read"), "unexpected error: {err}");

        // Run skips it (and reports it) rather than refusing to start.
        h.app.start_run();
        assert!(!h.app.runner.is_running());
        assert!(
            h.app.toasts.iter().any(|t| t.msg.contains("could not be read")
                || t.msg.contains("unreadable")),
            "no error toast for the unreadable file: {:?}",
            h.app.toasts.iter().map(|t| &t.msg).collect::<Vec<_>>()
        );
    }

    /// "New" rebuilds the default template and drops every per-node cache.
    #[test]
    fn new_workflow_restores_the_template() {
        let mut h = Harness::new();
        let node = h.app.editor.node_order[1];
        let (removed, _) = h.app.editor.graph.remove_node(node);
        h.app.editor.node_positions.remove(node);
        h.app.editor.node_order.retain(|n| *n != node);
        h.app
            .node_deleted(removed.user_data.json_id, false);
        h.app.editor.selected_nodes = h.app.editor.node_order.clone();
        h.app.tasks.push(TaskRow {
            id: 1,
            input: PathBuf::from("a.mp4"),
            output: PathBuf::from("a_fflow.mp4"),
            status: TaskStatus::Done,
            pct: 100.0,
            speed: String::new(),
            out_time: 1.0,
            duration: 1.0,
            error: None,
            secs: 1.0,
            expanded: false,
        });

        h.app.new_workflow();
        for _ in 0..2 {
            h.frame(Vec::new());
        }

        let expected = crate::graph::default_workflow(None).nodes.len();
        assert_eq!(h.app.editor.graph.nodes.len(), expected);
        assert_eq!(h.app.editor.node_order.len(), expected);
        assert!(h.app.editor.selected_nodes.is_empty());
        assert!(h.app.tasks.is_empty());
        assert!(h.app.ctx.source_counts.len() <= 1, "stale per-node caches survived");
        assert!(h.app.compile_current().is_err(), "no files yet");
    }

    /// Delete/Backspace removes the selected node, Escape clears the selection.
    #[test]
    fn keyboard_delete_and_escape() {
        let mut h = Harness::new();
        let before = h.app.editor.graph.nodes.len();
        let node = h.app.editor.node_order[3];
        h.click(h.node_rect(node).left_top() + egui::vec2(30.0, 9.0));
        h.frame(Vec::new());
        assert_eq!(h.app.editor.selected_nodes, vec![node]);

        h.key(egui::Key::Delete);
        h.frame(Vec::new());
        assert!(!h.app.editor.node_order.contains(&node));
        assert_eq!(h.app.editor.graph.nodes.len(), before - 1);
        assert!(h.app.editor.selected_nodes.is_empty());
        // The rest of the frame must keep working on the shrunken graph.
        let _ = h.app.compile_current();

        // Escape drops the selection without touching the graph.
        let other = h.app.editor.node_order[0];
        h.click(h.node_rect(other).left_top() + egui::vec2(30.0, 9.0));
        h.frame(Vec::new());
        assert_eq!(h.app.editor.selected_nodes, vec![other]);
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.editor.selected_nodes.is_empty());
        assert_eq!(h.app.editor.graph.nodes.len(), before - 1);
    }

    /// `SidePanel::exact_width` is only exact while nothing inside overflows.
    ///
    /// A row of two truncating labels in the file card used to be 12 px wider
    /// than the card, which widened the panel's *response* rect without
    /// widening its painted frame — the 12 px in between showed the window's
    /// black clear colour as a stripe down the canvas edge.
    #[test]
    fn panels_are_exactly_as_wide_as_requested() {
        let dir = std::env::temp_dir().join("fflow_ui_wide");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["185期-otto高清蓝光胶水.mp4", "14.mp4", "220.mp4"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let mut h = Harness::new();
        h.app.settings.panels.library = 305.0;
        h.app.settings.panels.inspector = 280.0;
        h.app.settings.panels.files = 173.0;
        h.app.demo_setup(Some(&dir), None).unwrap();
        for _ in 0..4 {
            h.frame(Vec::new());
        }
        let r = h.app.panel_rects;
        assert!(
            (r.library.width() - h.app.settings.panels.library).abs() < 0.5,
            "library is {:.1} px wide, asked for {:.1}",
            r.library.width(),
            h.app.settings.panels.library
        );
        assert!(
            (r.inspector.width() - h.app.settings.panels.inspector).abs() < 0.5,
            "inspector is {:.1} px wide, asked for {:.1}",
            r.inspector.width(),
            h.app.settings.panels.inspector
        );
        // …and nothing may be painted in the gap between the panels.
        assert!(
            (h.app.canvas_rect.left() - r.library.right()).abs() < 0.5,
            "canvas starts at {:.1}, library ends at {:.1}",
            h.app.canvas_rect.left(),
            r.library.right()
        );
        assert!(
            (h.app.canvas_rect.right() - r.inspector.left()).abs() < 0.5,
            "canvas ends at {:.1}, inspector starts at {:.1}",
            h.app.canvas_rect.right(),
            r.inspector.left()
        );
    }

    /// The node list is clipped at its bottom edge, so it has to stop short of
    /// the divider: otherwise the splitter line is painted through the middle
    /// of the last visible card.
    #[test]
    fn node_list_stops_above_the_file_divider() {
        let h = Harness::new();
        let list = h.strip_rect("split-files");
        let content = h.app.panel_rects.library_content;
        assert!(
            list.top() > content.top() + LIBRARY_MIN_NODES,
            "divider at {:.0} leaves no room for the list",
            list.top()
        );
        // The list's own scroll area must end above the line by the gutter.
        let list_bottom = h.app.panel_rects.files.top();
        assert!(
            (list_bottom - list.center().y).abs() < 1.0,
            "the divider sits at {:.1}, the file panel starts at {:.1}",
            list.center().y,
            list_bottom
        );
    }

    /// Every file row has to fit inside the file panel, or its labels push the
    /// panel (and the library) wider than the splitter thinks they are. The
    /// long CJK name is the one that used to overflow.
    #[test]
    fn file_rows_stay_inside_the_panel() {
        let dir = std::env::temp_dir().join("fflow_ui_rows");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "185期-otto高清蓝光胶水.mp4",
            "14.mp4",
            "220.mp4",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let mut h = Harness::new();
        h.app.demo_setup(Some(&dir), None).unwrap();
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        assert!(!h.app.files.is_empty(), "the temp dir has no media files");
        let files = h.app.panel_rects.files;
        let mut checked = 0;
        for i in 0..h.app.files.len() {
            let Some(resp) = h.ctx.read_response(file_card_id(i)) else {
                continue;
            };
            checked += 1;
            assert!(
                resp.rect.right() <= files.right() + 0.5,
                "file row {i} ends at {:.1}, panel at {:.1}",
                resp.rect.right(),
                files.right()
            );
        }
        assert!(checked > 0, "no file row was laid out");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    /// Symbols used in UI text (not painted, not icon-font-only).
    const SYMBOLS: &str = "·×—•…»›↻■▣▶◀⚠⚡";

    /// egui bundles Ubuntu-Light plus a monochrome NotoEmoji and nothing else:
    /// any other code point renders as a tofu box. That bit us with `◐`, `→`,
    /// `●`, `✕`, `▸`, `⑂` and `🎛`, all of which showed up as empty squares.
    /// Keep this list in sync with the labels.
    #[test]
    fn every_ui_glyph_renders() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));
        let _ = ctx.run(input, |ctx| {
            let font = egui::FontId::proportional(12.0);
            let mut missing = Vec::new();
            let mut check = |s: &str| {
                for c in s.chars() {
                    if !ctx.fonts(|f| f.has_glyph(&font, c)) {
                        missing.push(format!("U+{:04X} {c}", c as u32));
                    }
                }
            };
            check(SYMBOLS);
            for kind in NodeKind::ALL {
                check(icon_for(kind));
            }
            assert!(missing.is_empty(), "missing glyphs: {missing:?}");
        });
    }
}
