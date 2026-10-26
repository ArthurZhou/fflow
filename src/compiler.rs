//! Graph → ffmpeg command line.
//!
//! The compiler walks the graph backwards from the Package node, collects the
//! filter chain and encoders of each branch, and emits a single flat argument
//! vector that can be previewed verbatim in the UI.
//!
//! Two things make this more than a straight walk:
//!
//! * **Multi-track** — the Demux node may forward several video/audio streams.
//!   Each one becomes its own filter chain, its own `-map`, and the encoder
//!   flags are emitted once per stream type (ffmpeg applies `-c:v` to every
//!   output video stream).
//! * **Hardware substitution** — a software codec may be swapped for a verified
//!   hardware encoder, which changes the quality flags (`-cq` instead of
//!   `-crf`, …).

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::encoders::EncoderRegistry;
use crate::graph::nodes::{DemuxConfig, FileFacts, NodeConfig, NodeKind, PackageConfig};
use crate::graph::{package_nodes, FGraph};
use crate::probe::ProbeInfo;

/// One ffmpeg invocation. A job has several when a Package node feeds another
/// Demux (the intermediate file is written first, then read back).
#[derive(Debug, Clone)]
pub struct Stage {
    pub args: Vec<String>,
    pub output: PathBuf,
    pub display: String,
}

/// One fully compiled job for one input file.
#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub input: PathBuf,
    pub output: PathBuf,
    /// Arguments of the **last** stage (what the user mostly cares about).
    pub args: Vec<String>,
    pub display: String,
    pub duration: f64,
    pub notes: Vec<String>,
    /// Every stage in order; the last one writes `output`.
    pub stages: Vec<Stage>,
}

/// A compiled audio or video branch (one track).
#[derive(Debug, Default, Clone)]
struct StreamPlan {
    is_video: bool,
    /// 0-based ordinal within its type → `0:v:<track>`.
    track: u32,
    /// `[v]` / `[a]` / `[v1]` when filters were used, `None` → map the source
    /// stream directly.
    label: Option<String>,
    encoder_args: Vec<String>,
    filters: Vec<String>,
    notes: Vec<String>,
}

fn quote(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".into();
    }
    if arg
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '[' | ']' | ';' | '&' | '|'))
    {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

pub fn format_command(bin: &str, args: &[String]) -> String {
    let mut s = quote(bin);
    for a in args {
        s.push(' ');
        s.push_str(&quote(a));
    }
    s
}

/// First connection of an input port.
///
/// `Graph::connection` deliberately returns `None` for ports that allow more
/// than one connection (the merge points), so it cannot be used here.
fn first_conn(graph: &FGraph, input: egui_node_graph2::InputId) -> Option<egui_node_graph2::OutputId> {
    graph.connections(input).first().copied()
}

/// `[0:v]` for the first track, `[0:v:1]` for the second, …
fn source_label(is_video: bool, track: u32) -> String {
    let kind = if is_video { "v" } else { "a" };
    if track == 0 {
        format!("[0:{}]", kind)
    } else {
        format!("[0:{}:{}]", kind, track)
    }
}

/// Filter-graph output label. Single-track workflows keep the terse `[v]`/`[a]`
/// form so the preview stays readable.
fn output_label(is_video: bool, track: u32, total: usize) -> String {
    let kind = if is_video { "v" } else { "a" };
    if total <= 1 {
        format!("[{}]", kind)
    } else {
        format!("[{}{}]", kind, track)
    }
}

/// `-map` target when a branch has no filters.
fn map_target(is_video: bool, track: u32) -> String {
    format!("0:{}:{}", if is_video { "v" } else { "a" }, track)
}

/// Result of walking a branch backwards.
enum Upstream {
    /// Node chain, ordered Demux → … → Package.
    Chain(Vec<egui_node_graph2::NodeId>),
    /// A Condition node sent this file down a branch that is not wired up.
    Dropped(String),
}

/// Collect the node chain feeding `output`, ordered Demux → ... → Package.
///
/// The walk is recursive because a merge point (an encoder or the Package
/// node) may have several connections: a Condition's `pass` and `fail` sides
/// both re-join there. Each connection is walked with the file's facts, and
/// exactly one live branch must survive. Two unrelated live branches reaching
/// the same input is a real ambiguity, so it is reported instead of guessed.
fn upstream(
    graph: &FGraph,
    output: egui_node_graph2::OutputId,
    facts: &FileFacts<'_>,
) -> Result<Upstream> {
    upstream_at(graph, output, facts, 0)
}

fn upstream_at(
    graph: &FGraph,
    output: egui_node_graph2::OutputId,
    facts: &FileFacts<'_>,
    depth: usize,
) -> Result<Upstream> {
    if depth > 64 {
        bail!("filter chain too deep (cycle?)");
    }

    let node = graph.get_output(output).node;
    let kind = graph.nodes[node].user_data.kind();
    let port = graph.nodes[node]
        .outputs
        .iter()
        .find(|(_, id)| *id == output)
        .map(|(n, _)| n.clone())
        .unwrap_or_default();

    if kind == NodeKind::Demux {
        return Ok(Upstream::Chain(vec![node]));
    }
    if kind == NodeKind::Source || kind == NodeKind::Package {
        bail!("{} cannot appear inside a stream chain", kind.title());
    }

    // A Condition only lets one of its two outputs through.
    if kind == NodeKind::Condition {
        let matches = match &graph.nodes[node].user_data.config {
            NodeConfig::Condition(c) => c.matches(facts),
            _ => true,
        };
        let open = match port.as_str() {
            "pass" => matches,
            "fail" => !matches,
            _ => true,
        };
        if !open {
            return Ok(Upstream::Dropped(crate::tr!(
                "condition `{}` sent this file down its other branch",
                graph.nodes[node]
                    .user_data
                    .config
                    .summary()
                    .chars()
                    .take(40)
                    .collect::<String>(),
            )));
        }
    }

    let in_port = graph.nodes[node]
        .inputs
        .iter()
        .find(|(name, _)| name == "in")
        .map(|(_, id)| *id)
        .ok_or_else(|| anyhow::anyhow!("{} node has no 'in' port", kind.title()))?;
    let conns = graph.connections(in_port);
    if conns.is_empty() {
        return Ok(Upstream::Dropped(format!(
            "`{}` has nothing connected to its `in` port",
            kind.title()
        )));
    }

    // Walk every incoming branch; keep the ones that are live for this file.
    let mut live: Vec<Vec<egui_node_graph2::NodeId>> = Vec::new();
    let mut why = None;
    for conn in conns {
        match upstream_at(graph, conn, facts, depth + 1)? {
            Upstream::Chain(mut ch) => {
                ch.push(node);
                live.push(ch);
            }
            Upstream::Dropped(w) => why = Some(w),
        }
    }
    match live.len() {
        0 => Ok(Upstream::Dropped(
            why.unwrap_or_else(|| format!("`{}` received no live branch", kind.title())),
        )),
        1 => Ok(Upstream::Chain(live.pop().unwrap())),
        n => bail!(
            "{} has {} live branches for one file — connect a Condition's pass/fail \
             sides, or remove one of them",
            kind.title(),
            n
        ),
    }
}

/// One step of a video chain, in wiring order.
enum VideoStep {
    Filters(Vec<String>),
    Overlay {
        path: String,
        x: String,
        y: String,
        scale: f32,
    },
}

/// Build the filter_complex segments for one branch.
struct FilterGraph {
    segments: Vec<String>,
    extras: Vec<String>,
    counter: usize,
}

impl FilterGraph {
    fn new() -> Self {
        Self {
            segments: Vec::new(),
            extras: Vec::new(),
            counter: 0,
        }
    }

    fn label(&mut self) -> String {
        let l = format!("[t{}]", self.counter);
        self.counter += 1;
        l
    }

    fn extra(&mut self, path: &str) -> usize {
        // input 0 is the source file, extra inputs start at 1
        if let Some(pos) = self.extras.iter().position(|p| p == path) {
            return pos + 1;
        }
        self.extras.push(path.to_string());
        self.extras.len()
    }
}

fn build_video(
    fg: &mut FilterGraph,
    steps: &[VideoStep],
    src: &str,
    out: &str,
) -> Result<Option<String>> {
    if steps.is_empty() {
        return Ok(None);
    }
    let mut cur = src.to_string();
    let mut pending: Vec<String> = Vec::new();
    let mut finished = false;
    let mut any = false;
    let last_index = steps.len() - 1;

    for (i, step) in steps.iter().enumerate() {
        let last = i == last_index;
        match step {
            VideoStep::Filters(list) => {
                if !list.is_empty() {
                    any = true;
                    pending.extend(list.iter().cloned());
                }
            }
            VideoStep::Overlay {
                path,
                x,
                y,
                scale,
            } => {
                any = true;
                if !pending.is_empty() {
                    let lab = fg.label();
                    fg.segments
                        .push(format!("{}{}{}", cur, pending.join(","), lab));
                    cur = lab;
                    pending.clear();
                }
                let idx = fg.extra(path);
                // A non-unit scale needs the overlay input scaled first.
                let over = if (scale - 1.0).abs() > 1e-3 {
                    let lab = fg.label();
                    fg.segments
                        .push(format!("[{}:v]scale=iw*{}:-2{}", idx, scale, lab));
                    lab
                } else {
                    format!("[{}:v]", idx)
                };
                let lab = if last { out.to_string() } else { fg.label() };
                fg.segments.push(format!(
                    "{}{}overlay={}:{}:shortest=1{}",
                    cur, over, x, y, lab
                ));
                finished = last;
                cur = lab;
            }
        }
    }

    if !any {
        // Every card was a passthrough: map the source stream directly.
        return Ok(None);
    }
    if !finished {
        if pending.is_empty() {
            // Only overlays so far: alias the last temp label as the output.
            fg.segments.push(format!("{}null{}", cur, out));
        } else {
            fg.segments.push(format!("{}{}{}", cur, pending.join(","), out));
        }
    }
    Ok(Some(out.to_string()))
}

fn build_audio(
    fg: &mut FilterGraph,
    filters: &[String],
    src: &str,
    out: &str,
) -> Result<Option<String>> {
    if filters.is_empty() {
        // Passthrough only — let the compiler map the source stream.
        return Ok(None);
    }
    fg.segments
        .push(format!("{}{}{}", src, filters.join(","), out));
    Ok(Some(out.to_string()))
}

fn build_plan(
    graph: &FGraph,
    chain: &[egui_node_graph2::NodeId],
    is_video: bool,
    track: u32,
    total: usize,
    fg: &mut FilterGraph,
    reg: Option<&EncoderRegistry>,
) -> Result<StreamPlan> {
    // Filter cards are collected in wiring order: the order the user built on
    // the canvas is the order ffmpeg applies them in.
    let mut video_steps: Vec<VideoStep> = Vec::new();
    let mut audio_steps: Vec<String> = Vec::new();
    let mut encoder: Option<Vec<String>> = None;
    let mut notes: Vec<String> = Vec::new();

    for node in &chain[1..] {
        match &graph.nodes[*node].user_data.config {
            // Resolved while walking upstream; nothing to compile here.
            NodeConfig::Condition(_) => {}
            NodeConfig::VideoEncoder(c) => {
                if !is_video {
                    bail!("a Video Encoder is connected to the audio branch");
                }
                let (codec, note) = c.effective_codec(reg);
                if let Some(r) = reg {
                    if codec != "copy" && !r.is_usable(&codec) {
                        bail!(
                            "video encoder `{}` is not available in this ffmpeg build — \
                             pick another codec or press “detect encoders”",
                            codec
                        );
                    }
                }
                if let Some(n) = note {
                    notes.push(n);
                }
                encoder = Some(c.args_for(&codec));
            }
            NodeConfig::AudioEncoder(c) => {
                if is_video {
                    bail!("an Audio Encoder is connected to the video branch");
                }
                if let Some(r) = reg {
                    if c.codec != "copy" && !r.is_usable(&c.codec) {
                        bail!(
                            "audio encoder `{}` is not available in this ffmpeg build",
                            c.codec
                        );
                    }
                }
                encoder = Some(c.args());
            }
            NodeConfig::Source(_) | NodeConfig::Package(_) | NodeConfig::Demux(_) => {}
            other => {
                if is_video {
                    let filters = other.video_filters()?;
                    if !filters.is_empty() {
                        video_steps.push(VideoStep::Filters(filters));
                    }
                    if let Some((path, x, y, scale)) = other.overlay() {
                        video_steps.push(VideoStep::Overlay { path, x, y, scale });
                    }
                } else {
                    let filters = other.audio_filters()?;
                    audio_steps.extend(filters);
                }
            }
        }
    }

    let src = source_label(is_video, track);
    let out = output_label(is_video, track, total);
    let (label, filters) = if is_video {
        let used: Vec<String> = video_steps
            .iter()
            .flat_map(|s| match s {
                VideoStep::Filters(f) => f.clone(),
                VideoStep::Overlay { path, .. } => vec![format!("overlay={}", path)],
            })
            .collect();
        (build_video(fg, &video_steps, &src, &out)?, used)
    } else {
        (build_audio(fg, &audio_steps, &src, &out)?, audio_steps.clone())
    };

    let has_filters = label.is_some();
    let encoder_args = match encoder {
        Some(args) => args,
        None if has_filters => {
            bail!(
                "{} branch has filters but no encoder — add a {} Encoder node",
                if is_video { "video" } else { "audio" },
                if is_video { "Video" } else { "Audio" }
            )
        }
        // Pure remux of that branch.
        None => vec![
            if is_video { "-c:v" } else { "-c:a" }.to_string(),
            "copy".to_string(),
        ],
    };

    Ok(StreamPlan {
        is_video,
        track,
        label,
        encoder_args,
        filters,
        notes,
    })
}

/// The config of a specific Demux node.
fn demux_config(graph: &FGraph, node: egui_node_graph2::NodeId) -> Option<&DemuxConfig> {
    match &graph.nodes[node].user_data.config {
        NodeConfig::Demux(c) => Some(c),
        _ => None,
    }
}

/// Render the output path from the package template.
pub fn render_output_path(
    cfg: &PackageConfig,
    input: &Path,
    index: usize,
    root: Option<&Path>,
) -> PathBuf {
    let ext = cfg.ext();
    let dir = if cfg.output_dir.trim().is_empty() {
        input
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(cfg.output_dir.trim())
    };
    // A bare file name has an empty parent; keep it relative instead of
    // producing a path at the filesystem root.
    let dir_str = if dir.as_os_str().is_empty() {
        ".".to_string()
    } else {
        dir.to_string_lossy().to_string()
    };
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let name = input
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let rel_dir = root
        .and_then(|r| input.parent().and_then(|p| p.strip_prefix(r).ok()))
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    let mut tpl = cfg.name_template.trim().to_string();
    if tpl.is_empty() {
        tpl = "{dir}/{stem}_fflow.{ext}".into();
    }
    tpl = tpl
        .replace("{dir}", &dir_str)
        .replace("{stem}", &stem)
        .replace("{name}", &name)
        .replace("{rel_dir}", &rel_dir)
        .replace("{index}", &format!("{:03}", index))
        .replace("{ext}", ext);

    // Collapse accidental double separators introduced by empty tokens.
    while tpl.contains("//") {
        tpl = tpl.replace("//", "/");
    }
    PathBuf::from(tpl)
}

/// A file that a Condition node routed to an unconnected branch.
///
/// This is not a failure: the batch runner shows it as "skipped" and moves on.
#[derive(Debug)]
pub struct Skipped(pub String);

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Skipped {}

/// Was this compile error a condition skip rather than a real failure?
pub fn skipped_reason(err: &anyhow::Error) -> Option<&str> {
    err.downcast_ref::<Skipped>().map(|s| s.0.as_str())
}

/// One compiled stage: a Demux → … → Package pass over one file.
struct StageCompiled {
    args: Vec<String>,
    display: String,
    plans: Vec<StreamPlan>,
    notes: Vec<String>,
}

/// Compile one Demux → Package pass.
///
/// `input` is the file this stage reads (the source file, or the intermediate
/// written by the previous stage) and `out_path` is where it writes.
fn compile_stage(
    graph: &FGraph,
    demux: egui_node_graph2::NodeId,
    pkg: egui_node_graph2::NodeId,
    input: &Path,
    info: &ProbeInfo,
    out_path: &Path,
) -> Result<StageCompiled> {
    let pkg_cfg = match &graph.nodes[pkg].user_data.config {
        NodeConfig::Package(c) => c,
        _ => unreachable!("stage package is a Package node"),
    };

    let reg = crate::encoders::snapshot();
    let reg_ref = reg.as_deref();

    // Hand-built ProbeInfo (tests, cached data) may lack the stream vectors;
    // fall back to the boolean summary so a single track still works.
    let v_total = if info.video_streams.is_empty() {
        usize::from(info.has_video)
    } else {
        info.video_streams.len()
    };
    let a_total = if info.audio_streams.is_empty() {
        usize::from(info.has_audio)
    } else {
        info.audio_streams.len()
    };
    let s_total = info.subtitle_streams.len();

    let (v_ords, a_ords, s_ords) = match demux_config(graph, demux) {
        Some(d) => (
            d.video_mode.resolve(&d.video_tracks, v_total),
            d.audio_mode.resolve(&d.audio_tracks, a_total),
            d.subtitle_mode.resolve(&d.subtitle_tracks, s_total),
        ),
        None => (
            if v_total > 0 { vec![0] } else { Vec::new() },
            if a_total > 0 { vec![0] } else { Vec::new() },
            Vec::new(),
        ),
    };

    let mut fg = FilterGraph::new();
    let mut plans: Vec<StreamPlan> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // One condition evaluation per file, shared by both branches.
    let size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let facts = FileFacts {
        path: input,
        size,
        info,
    };

    // A Condition in front of the Demux decides whether this file takes this
    // branch at all; the stream-chain walk below never sees it.
    check_gates(graph, demux, &facts)?;

    for (port, is_video) in [("video", true), ("audio", false)] {
        let Some(port_id) = graph.nodes[pkg]
            .inputs
            .iter()
            .find(|(n, _)| n == port)
            .map(|(_, id)| *id)
        else {
            continue;
        };
        let Some(conn) = first_conn(graph, port_id) else {
            continue;
        };

        let (total, ords) = if is_video {
            (v_total, &v_ords)
        } else {
            (a_total, &a_ords)
        };
        if total == 0 {
            notes.push(format!(
                "input has no {} stream — branch skipped",
                if is_video { "video" } else { "audio" }
            ));
            continue;
        }
        if ords.is_empty() {
            notes.push(format!(
                "{} branch skipped: no {} track selected in the Demux node",
                if is_video { "video" } else { "audio" },
                if is_video { "video" } else { "audio" }
            ));
            continue;
        }
        if ords.len() < total {
            notes.push(format!(
                "{}/{} {} track(s) forwarded",
                ords.len(),
                total,
                if is_video { "video" } else { "audio" }
            ));
        }

        let chain = match upstream(graph, conn, &facts)? {
            Upstream::Chain(c) => c,
            Upstream::Dropped(why) => {
                // A condition dropped this branch; if the other branch is
                // dropped too the whole file is skipped below.
                notes.push(why);
                continue;
            }
        };
        for &track in ords {
            plans.push(build_plan(
                graph,
                &chain,
                is_video,
                track,
                ords.len(),
                &mut fg,
                reg_ref,
            )?);
        }
    }

    if plans.is_empty() {
        if let Some(why) = notes.iter().find(|n| n.contains("condition")) {
            return Err(anyhow::Error::new(Skipped(why.clone())));
        }
        bail!(
            "nothing to encode for {} — connect a Demux branch to the Package node",
            input.display()
        );
    }

    let filter_complex = if fg.segments.is_empty() {
        None
    } else {
        Some(fg.segments.join(";"))
    };

    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostats".into(),
        "-loglevel".into(),
        "error".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
    ];

    for extra in &fg.extras {
        args.push("-i".into());
        args.push(extra.clone());
    }

    if let Some(fc) = &filter_complex {
        args.push("-filter_complex".into());
        args.push(fc.clone());
    }

    for plan in &plans {
        args.push("-map".into());
        args.push(match &plan.label {
            Some(label) => label.clone(),
            None => map_target(plan.is_video, plan.track),
        });
    }

    // `-c:v` / `-c:a` apply to every output stream of that type, so emit each
    // set once even when several tracks share the same encoder.
    let (mut video_done, mut audio_done) = (false, false);
    for plan in &plans {
        let done = if plan.is_video {
            &mut video_done
        } else {
            &mut audio_done
        };
        if *done {
            continue;
        }
        *done = true;
        args.extend(plan.encoder_args.iter().cloned());
    }

    // Subtitles bypass the filter graph entirely.
    if !s_ords.is_empty() {
        if pkg_cfg.supports_subtitles() {
            for s in &s_ords {
                args.push("-map".into());
                args.push(format!("0:s:{}", s));
            }
            args.push("-c:s".into());
            args.push(pkg_cfg.subtitle_codec().to_string());
            notes.push(format!(
                "{} subtitle track(s) mapped as {}",
                s_ords.len(),
                pkg_cfg.subtitle_codec()
            ));
        } else {
            notes.push(format!(
                "container `{}` cannot carry subtitles — {} track(s) dropped",
                pkg_cfg.container,
                s_ords.len()
            ));
        }
    }

    args.extend(pkg_cfg.args());

    let output = out_path.to_path_buf();
    args.push(output.to_string_lossy().to_string());

    let display = format_command(&crate::probe::ffmpeg_bin(), &args);
    let notes_all: Vec<String> = notes
        .into_iter()
        .chain(plans.iter().flat_map(|p| p.notes.iter().cloned()))
        .chain(
            plans
                .iter()
                .flat_map(|p| p.filters.iter().map(|f| format!("filter: {}", f))),
        )
        .collect();

    Ok(StageCompiled {
        args,
        display,
        plans,
        notes: notes_all,
    })
}

/// Compile the whole graph for a single input file.
///
/// A Package node may feed another Demux, which means "package this, then keep
/// processing the result". Each hop becomes a stage; the intermediate files go
/// to a temp directory and are removed by the runner when the job is done.
/// Compile one file into **every** output the graph asks for.
///
/// A workflow with a single terminal Package yields exactly one job, which is
/// the common case. Several terminals mean the file is encoded once per
/// output; a branch whose Condition rejects the file simply contributes no
/// job, and the file only counts as skipped when *every* branch skips it.
pub fn compile_all(
    graph: &FGraph,
    input: &Path,
    info: &ProbeInfo,
    index: usize,
    root: Option<&Path>,
) -> Result<Vec<Job>> {
    let plans = stage_plans(graph)?;
    let mut jobs = Vec::new();
    // Keep the first skip so a file that matches no branch reports the same
    // "skipped — <reason>" it always did.
    let mut skipped: Option<anyhow::Error> = None;
    let mut failure: Option<anyhow::Error> = None;

    for (slot, chain) in plans.iter().enumerate() {
        match compile_chain(graph, chain, input, info, index, root, slot) {
            Ok(job) => jobs.push(job),
            Err(e) if skipped_reason(&e).is_some() => {
                if skipped.is_none() {
                    skipped = Some(e);
                }
            }
            Err(e) => {
                if failure.is_none() {
                    failure = Some(e);
                }
            }
        }
    }

    if jobs.is_empty() {
        // A real error outranks a skip: if one branch is misconfigured the
        // user needs to know, even when another branch would have skipped.
        return Err(failure.or(skipped).unwrap_or_else(|| {
            anyhow::anyhow!("no Package node produced an output for this file")
        }));
    }

    // Two Packages with the same container and template would fight over one
    // path; ffmpeg would silently let the last one win.
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for job in &jobs {
        if !seen.insert(job.output.clone()) {
            bail!(
                "two Package nodes both write {}; give them different naming templates or containers",
                job.output.display()
            );
        }
    }

    Ok(jobs)
}

/// Compile one terminal Package's chain for one file.
fn compile_chain(
    graph: &FGraph,
    chain: &[(egui_node_graph2::NodeId, egui_node_graph2::NodeId)],
    input: &Path,
    info: &ProbeInfo,
    index: usize,
    root: Option<&Path>,
    slot: usize,
) -> Result<Job> {
    let total = chain.len();

    let mut stages: Vec<Stage> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut cur_input = input.to_path_buf();
    let mut cur_info = info.clone();

    for (i, (pkg, demux)) in chain.iter().enumerate() {
        let last = i + 1 == total;
        let out_path = if last {
            let cfg = match &graph.nodes[*pkg].user_data.config {
                NodeConfig::Package(c) => c,
                _ => unreachable!(),
            };
            let p = render_output_path(cfg, input, index, root);
            if p == input {
                bail!(
                    "output path would overwrite the source file ({}); change the naming template",
                    input.display()
                );
            }
            p
        } else {
            let cfg = match &graph.nodes[*pkg].user_data.config {
                NodeConfig::Package(c) => c,
                _ => unreachable!(),
            };
            // `slot` keeps the temp files of two branches apart when they run
            // for the same file.
            intermediate_path(input, index, slot * 100 + i, cfg.ext())
        };

        let st = compile_stage(graph, *demux, *pkg, &cur_input, &cur_info, &out_path)?;
        if !last {
            notes.push(format!(
                "stage {} writes {} (temporary, removed when the job ends)",
                i + 1,
                out_path.display()
            ));
        }
        notes.extend(st.notes.iter().cloned());
        cur_info = predict_info(&cur_info, &st.plans, &out_path);
        stages.push(Stage {
            args: st.args,
            output: out_path.clone(),
            display: st.display,
        });
        cur_input = out_path;
    }

    let last_stage = stages.last().expect("at least one stage");
    let output = last_stage.output.clone();
    let args = last_stage.args.clone();
    let display = if stages.len() == 1 {
        last_stage.display.clone()
    } else {
        stages
            .iter()
            .enumerate()
            .map(|(i, s)| format!("[{}] {}", i + 1, s.display))
            .collect::<Vec<_>>()
            .join("\n")
    };

    Ok(Job {
        id: index,
        output,
        input: input.to_path_buf(),
        args,
        display,
        duration: info.duration,
        notes,
        stages,
    })
}

/// Compile one file for a single output — the common single-Package case.
pub fn compile(
    graph: &FGraph,
    input: &Path,
    info: &ProbeInfo,
    index: usize,
    root: Option<&Path>,
) -> Result<Job> {
    let jobs = compile_all(graph, input, info, index, root)?;
    let first = jobs.into_iter().next().expect("compile_all returns ≥1 job");
    Ok(first)
}

/// Where an intermediate stage writes: a per-batch temp file.
fn intermediate_path(input: &Path, index: usize, stage: usize, ext: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let safe: String = stem
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    std::env::temp_dir()
        .join("fflow")
        .join(format!("{}-{:04}-s{}.{}", safe, index, stage + 1, ext))
}

/// Predict the streams an intermediate file will contain, so the next stage can
/// resolve "first track" / "all tracks" before the file exists.
fn predict_info(prev: &ProbeInfo, plans: &[StreamPlan], path: &Path) -> ProbeInfo {
    let mut p = ProbeInfo {
        path: path.to_string_lossy().to_string(),
        duration: prev.duration,
        tags: prev.tags.clone(),
        // Subtitles are copied through by the stage that produced the file.
        subtitle_streams: prev.subtitle_streams.clone(),
        ..Default::default()
    };

    let codec_of = |plan: &StreamPlan| -> String {
        plan.encoder_args
            .iter()
            .position(|a| a == "-c:v" || a == "-c:a")
            .and_then(|i| plan.encoder_args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| "copy".into())
    };

    for plan in plans {
        let mut st = crate::probe::StreamInfo {
            ordinal: if plan.is_video {
                p.video_streams.len() as u32
            } else {
                p.audio_streams.len() as u32
            },
            codec: codec_of(plan),
            ..Default::default()
        };
        if plan.is_video {
            st.width = prev.width;
            st.height = prev.height;
            st.fps = prev.fps;
            p.video_streams.push(st);
        } else {
            st.channels = prev.audio_streams.first().map(|s| s.channels).unwrap_or(2);
            st.sample_rate = prev
                .audio_streams
                .first()
                .map(|s| s.sample_rate)
                .unwrap_or(48000);
            p.audio_streams.push(st);
        }
    }
    p.has_video = !p.video_streams.is_empty();
    p.has_audio = !p.audio_streams.is_empty();
    p
}

/// What feeds a Demux once the Conditions in front of it are skipped over.
struct Routing {
    /// The Source or Package at the top of this branch, if any.
    source: Option<egui_node_graph2::NodeId>,
    /// Conditions between that node and the Demux, with the output port the
    /// branch left them by — `pass` or `fail`.
    gates: Vec<(egui_node_graph2::NodeId, String)>,
}

/// Walk `in` ports upstream from a Demux, through any Condition nodes.
///
/// A workflow is allowed to put a Condition *before* the Demux
/// (Source → Condition → Demux), which routes whole files rather than streams.
/// The filter-chain walk stops at the Demux, so those gates have to be found
/// separately: the planner needs the node at the top, and the compiler needs
/// the gates to decide whether this file takes the branch at all.
fn routing(graph: &FGraph, demux: egui_node_graph2::NodeId) -> Routing {
    let mut gates = Vec::new();
    let mut cur = demux;
    for _ in 0..16 {
        let Some(in_port) = graph.nodes[cur]
            .inputs
            .iter()
            .find(|(n, _)| n == "in")
            .map(|(_, id)| *id)
        else {
            break;
        };
        let Some(conn) = first_conn(graph, in_port) else {
            break;
        };
        let src = graph.get_output(conn).node;
        if graph.nodes[src].user_data.kind() != NodeKind::Condition {
            return Routing {
                source: Some(src),
                gates,
            };
        }
        let port = graph.nodes[src]
            .outputs
            .iter()
            .find(|(_, id)| *id == conn)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        gates.push((src, port));
        cur = src;
    }
    Routing {
        source: None,
        gates,
    }
}

/// Reject the file if any gate in front of the Demux sent it elsewhere.
fn check_gates(graph: &FGraph, demux: egui_node_graph2::NodeId, facts: &FileFacts<'_>) -> Result<()> {
    for (node, port) in routing(graph, demux).gates {
        let matches = match &graph.nodes[node].user_data.config {
            NodeConfig::Condition(c) => c.matches(facts),
            _ => true,
        };
        let open = match port.as_str() {
            "pass" => matches,
            "fail" => !matches,
            _ => true,
        };
        if !open {
            return Err(anyhow::Error::new(Skipped(crate::tr!(
                "condition `{}` sent this file down its other branch",
                graph.nodes[node]
                    .user_data
                    .config
                    .summary()
                    .chars()
                    .take(40)
                    .collect::<String>(),
            ))));
        }
    }
    Ok(())
}

/// Every output of the graph: one chain per terminal Package, first stage →
/// last stage.
///
/// A workflow may end in several Packages — one file can be encoded to mp4 for
/// the video branch and to wav for the audio branch — so each terminal is
/// planned separately and becomes its own job. Packages whose `file` output is
/// wired into something are mid-chain stages, not outputs.
fn stage_plans(
    graph: &FGraph,
) -> Result<Vec<Vec<(egui_node_graph2::NodeId, egui_node_graph2::NodeId)>>> {
    let packages = package_nodes(graph);
    if packages.is_empty() {
        bail!("the workflow has no Package node");
    }

    let terminals: Vec<_> = packages
        .iter()
        .copied()
        .filter(|p| !file_output_connected(graph, *p))
        .collect();
    if terminals.is_empty() {
        bail!("every Package node feeds another node — the workflow has no output");
    }

    let mut plans = Vec::with_capacity(terminals.len());
    for pkg in terminals {
        plans.push(stage_chain(graph, pkg)?);
    }
    Ok(plans)
}

/// Walk one Package back through the Demuxes that feed it.
fn stage_chain(
    graph: &FGraph,
    mut pkg: egui_node_graph2::NodeId,
) -> Result<Vec<(egui_node_graph2::NodeId, egui_node_graph2::NodeId)>> {
    let mut chain = Vec::new();
    for _ in 0..16 {
        let demux = structural_demux(graph, pkg)?;
        chain.push((pkg, demux));

        let in_port = graph.nodes[demux]
            .inputs
            .iter()
            .find(|(n, _)| n == "in")
            .map(|(_, id)| *id)
            .ok_or_else(|| anyhow::anyhow!("the Demux node has no `in` port"))?;
        let Some(conn) = first_conn(graph, in_port) else {
            bail!("the Demux node has nothing connected to its `in` port");
        };
        let src = graph.get_output(conn).node;
        // Skip over Conditions: they route files, not streams, and are
        // evaluated per file when the stage is compiled.
        let src = routing(graph, demux).source.unwrap_or(src);
        match graph.nodes[src].user_data.kind() {
            NodeKind::Package => pkg = src,
            NodeKind::Source => {
                chain.reverse();
                return Ok(chain);
            }
            other => bail!(
                "a Demux must be fed by a Source or a Package node, not by {}",
                other.title()
            ),
        }
    }
    bail!("the workflow chains more than 16 stages — is there a loop?")
}

/// Is this Package's `file` output wired into something?
fn file_output_connected(graph: &FGraph, pkg: egui_node_graph2::NodeId) -> bool {
    let Some(out) = graph.nodes[pkg]
        .outputs
        .iter()
        .find(|(n, _)| n == "file")
        .map(|(_, id)| *id)
    else {
        return false;
    };
    graph.iter_nodes().any(|n| {
        graph.nodes[n]
            .inputs
            .iter()
            .any(|(_, input)| graph.connections(*input).contains(&out))
    })
}

/// The Demux a Package reads from, ignoring conditions.
fn structural_demux(graph: &FGraph, pkg: egui_node_graph2::NodeId) -> Result<egui_node_graph2::NodeId> {
    let mut stack: Vec<egui_node_graph2::NodeId> = graph.nodes[pkg]
        .inputs
        .iter()
        .filter_map(|(_, id)| first_conn(graph, *id))
        .map(|o| graph.get_output(o).node)
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut found: Option<egui_node_graph2::NodeId> = None;

    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        let kind = graph.nodes[node].user_data.kind();
        if kind == NodeKind::Demux {
            if let Some(prev) = found {
                if prev != node {
                    bail!(
                        "the Package node reads from two different Demux nodes; \
                         a stage must come from one file"
                    );
                }
            }
            found = Some(node);
            continue;
        }
        if kind == NodeKind::Source || kind == NodeKind::Package {
            continue;
        }
        for (_, input) in &graph.nodes[node].inputs {
            for conn in graph.connections(*input) {
                stack.push(graph.get_output(conn).node);
            }
        }
    }

    found.ok_or_else(|| {
        anyhow::anyhow!(crate::i18n::t(
            "the Package node is not connected to a Demux — add one between the source and the encoders"
        ))
    })
}

/// What happens to one file, without running ffmpeg.
///
/// This answers "does my wiring do what I think?" — the canvas dims everything
/// that is not in `nodes`/`links`, so a misplaced Condition shows up as a
/// branch that never lights up.
#[derive(Debug, Default, Clone)]
pub struct Trace {
    /// Nodes this file flows through, across every output that takes it.
    pub nodes: std::collections::HashSet<egui_node_graph2::NodeId>,
    /// Connections this file flows through.
    pub links: std::collections::HashSet<(egui_node_graph2::OutputId, egui_node_graph2::InputId)>,
    /// One entry per terminal Package, in graph order.
    pub outputs: Vec<OutputTrace>,
}

impl Trace {
    /// Did any output take this file?
    pub fn flows(&self) -> bool {
        self.outputs.iter().any(|o| o.taken)
    }

    /// The outputs this file will actually be written to.
    pub fn taken(&self) -> impl Iterator<Item = &OutputTrace> {
        self.outputs.iter().filter(|o| o.taken)
    }
}

/// One output's verdict for one file.
#[derive(Debug, Clone)]
pub struct OutputTrace {
    pub package: egui_node_graph2::NodeId,
    /// Where this output writes, for the summary.
    pub target: String,
    pub taken: bool,
    /// Why this output did not take the file (a closed Condition, usually).
    pub reason: Option<String>,
    /// Node titles from the Source to the Package, in flow order. Branches that
    /// split at the Demux are concatenated, so this is the list of nodes the
    /// file passes through rather than a drawn-out pipeline — the pipeline
    /// itself is on the canvas, which is the point of the preview.
    pub path: Vec<String>,
}

/// Append a node's title to the summary path unless it is already there.
///
/// Keyed on the node id, not on the title: two Packages are both "Package",
/// and a branch that re-joins at the Demux must not repeat what came before it.
fn push_node(
    graph: &FGraph,
    path: &mut Vec<String>,
    seen: &mut Vec<egui_node_graph2::NodeId>,
    id: egui_node_graph2::NodeId,
) {
    if seen.contains(&id) {
        return;
    }
    seen.push(id);
    path.push(graph.nodes[id].user_data.kind().title().to_string());
}

/// The connection from `from`'s output to `to`'s input, when they are wired.
fn link_between(
    graph: &FGraph,
    from: egui_node_graph2::NodeId,
    to: egui_node_graph2::NodeId,
) -> Option<(egui_node_graph2::OutputId, egui_node_graph2::InputId)> {
    for (_, input) in &graph.nodes[to].inputs {
        for conn in graph.connections(*input) {
            if graph.get_output(conn).node == from {
                return Some((conn, *input));
            }
        }
    }
    None
}

/// Decide what the graph does with one file, using the same checks the
/// compiler runs: the gates in front of each Demux, then the Condition-aware
/// walk up each connected Package input.
///
/// Deliberately built on `routing`, `check_gates` and `upstream` rather than a
/// second implementation of the flow rules — a preview that disagrees with the
/// compiler would be worse than no preview.
pub fn trace(graph: &FGraph, input: &Path, info: &ProbeInfo) -> Trace {
    let mut t = Trace::default();
    let Ok(plans) = stage_plans(graph) else {
        // A graph with no usable output has nothing to preview; the command
        // panel already shows the compile error.
        return t;
    };

    let size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let facts = FileFacts {
        path: input,
        size,
        info,
    };

    for chain in plans {
        let (pkg, _) = *chain.last().expect("a chain is never empty");
        let target = match &graph.nodes[pkg].user_data.config {
            NodeConfig::Package(c) => c.ext().to_string(),
            _ => String::new(),
        };
        let mut verdict = OutputTrace {
            package: pkg,
            target,
            taken: false,
            reason: None,
            path: Vec::new(),
        };

        // Every stage of the chain has to let the file through.
        let mut blocked = None;
        for (_, demux) in &chain {
            if let Err(e) = check_gates(graph, *demux, &facts) {
                blocked = Some(
                    skipped_reason(&e)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("{:#}", e)),
                );
                break;
            }
        }
        if let Some(why) = blocked {
            verdict.reason = Some(why);
            t.outputs.push(verdict);
            continue;
        }

        // Walk each connected input of the terminal Package.
        let mut live = false;
        let mut dropped: Option<String> = None;
        // A Package usually has a video *and* an audio input, both fed from the
        // same Source and Demux. The summary is a list of nodes the file passes
        // through, so each node is listed once, in first-seen order.
        let mut seen: Vec<egui_node_graph2::NodeId> = Vec::new();

        for (_, port_id) in graph.nodes[pkg].inputs.clone() {
            let Some(conn) = first_conn(graph, port_id) else {
                continue;
            };
            match upstream(graph, conn, &facts) {
                Ok(Upstream::Chain(nodes)) => {
                    live = true;
                    t.links.insert((conn, port_id));
                    for n in &nodes {
                        t.nodes.insert(*n);
                    }
                    // The chain runs Demux → … → the node feeding the port, so
                    // each pair is (upstream, downstream).
                    for pair in nodes.windows(2) {
                        if let Some(link) = link_between(graph, pair[0], pair[1]) {
                            t.links.insert(link);
                        }
                    }
                    // Conditions in front of the Demux route whole files, so
                    // they are on the path too — dimming a gate that the file
                    // just went through would be a lie.
                    let r = routing(graph, nodes[0]);
                    if let Some(src) = r.source {
                        t.nodes.insert(src);
                        let mut below = nodes[0];
                        for (gate, _) in &r.gates {
                            t.nodes.insert(*gate);
                            if let Some(link) = link_between(graph, *gate, below) {
                                t.links.insert(link);
                            }
                            below = *gate;
                        }
                        if let Some(link) = link_between(graph, src, below) {
                            t.links.insert(link);
                        }
                        push_node(graph, &mut verdict.path, &mut seen, src);
                        for (gate, _) in r.gates.iter().rev() {
                            push_node(graph, &mut verdict.path, &mut seen, *gate);
                        }
                    }
                    for n in &nodes {
                        push_node(graph, &mut verdict.path, &mut seen, *n);
                    }
                }
                Ok(Upstream::Dropped(why)) => dropped = Some(why),
                Err(e) => dropped = Some(format!("{:#}", e)),
            }
        }

        if live {
            verdict.taken = true;
            t.nodes.insert(pkg);
            push_node(graph, &mut verdict.path, &mut seen, pkg);
        } else {
            verdict.reason = Some(
                dropped.unwrap_or_else(|| crate::i18n::t("nothing connected to this Package").to_string()),
            );
        }
        t.outputs.push(verdict);
    }

    t
}

/// Convenience wrapper used by the preview panel and by `--selftest`.
///
/// One entry per file; a file can carry several jobs when the workflow has
/// more than one terminal Package.
pub fn compile_batch(
    graph: &FGraph,
    files: &[PathBuf],
    probes: &std::collections::HashMap<String, ProbeInfo>,
    root: Option<&Path>,
) -> Vec<Result<Vec<Job>>> {
    // Ids have to be unique across the whole batch: a file with two outputs
    // yields two jobs, and the runner keys its progress events on the id.
    let mut next_id = 1usize;
    files
        .iter()
        .enumerate()
        .map(|(i, f)| match probes.get(&f.to_string_lossy().to_string()) {
            Some(info) => compile_all(graph, f, info, i + 1, root).map(|mut jobs| {
                for job in &mut jobs {
                    job.id = next_id;
                    next_id += 1;
                }
                jobs
            }),
            None => Err(anyhow::anyhow!("no probe data for {}", f.display())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::nodes::{PackageConfig, TrackMode, VideoEncoderConfig};

    fn probe(v: usize, a: usize, s: usize) -> ProbeInfo {
        use crate::probe::StreamInfo;
        ProbeInfo {
            has_video: v > 0,
            has_audio: a > 0,
            duration: 1.0,
            video_streams: (0..v)
                .map(|i| StreamInfo {
                    ordinal: i as u32,
                    codec: "h264".into(),
                    width: 640,
                    height: 360,
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
                    language: if i == 1 { "eng".into() } else { "jpn".into() },
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

    /// Default pipeline, optionally with an Audio Filter spliced between the
    /// Demux node and the audio encoder (so the audio branch has a filter
    /// chain and therefore real `[0:a:N]` labels).
    fn workflow_with(d: DemuxConfig, audio_filter: bool) -> crate::graph::Workflow {
        use crate::graph::model::{Edge, WfNode};
        let mut wf = crate::graph::default_workflow(None);
        for n in &mut wf.nodes {
            if n.kind == NodeKind::Demux.id() {
                n.config = serde_json::to_value(&d).unwrap();
            }
        }
        // A Scale card keeps the video branch filtered, so the tests exercise
        // the `[v]` labels rather than a direct stream map.
        {
            let demux = wf.nodes.iter().find(|n| n.kind == NodeKind::Demux.id()).unwrap().id;
            let enc = wf.nodes.iter().find(|n| n.kind == NodeKind::VideoEncoder.id()).unwrap().id;
            wf.nodes.push(WfNode {
                id: 7,
                kind: NodeKind::Scale.id().into(),
                config: serde_json::to_value(crate::graph::nodes::ScaleConfig {
                    width: "1280".into(),
                    height: "-2".into(),
                    mode: crate::graph::nodes::ScaleMode::Exact,
                })
                .unwrap(),
                pos: None,
            });
            wf.edges
                .retain(|e| !(e.from.0 == demux && e.from.1 == "video"));
            wf.edges.push(Edge {
                from: (demux, "video".into()),
                to: (7, "in".into()),
            });
            wf.edges.push(Edge {
                from: (7, "out".into()),
                to: (enc, "in".into()),
            });
        }
        if audio_filter {
            let demux = wf.nodes.iter().find(|n| n.kind == NodeKind::Demux.id()).unwrap().id;
            let enc = wf.nodes.iter().find(|n| n.kind == NodeKind::AudioEncoder.id()).unwrap().id;
            // A Volume card with a non-zero gain, so the per-track labels are
            // exercised (the default 0 dB card is a passthrough).
            let cfg = crate::graph::nodes::VolumeConfig { db: 3.0 };
            wf.nodes.push(WfNode {
                id: 6,
                kind: NodeKind::Volume.id().into(),
                config: serde_json::to_value(cfg).unwrap(),
                pos: None,
            });
            wf.edges
                .retain(|e| !(e.from.0 == demux && e.from.1 == "audio"));
            wf.edges.push(Edge {
                from: (demux, "audio".into()),
                to: (6, "in".into()),
            });
            wf.edges.push(Edge {
                from: (6, "out".into()),
                to: (enc, "in".into()),
            });
        }
        wf
    }

    fn editor_with_demux(d: DemuxConfig) -> crate::graph::GraphEditor {
        editor_from(workflow_with(d, false))
    }

    fn editor_from(wf: crate::graph::Workflow) -> crate::graph::GraphEditor {
        let mut ctx = crate::graph::GraphCtx::default();
        crate::graph::from_workflow(&wf, &mut ctx).unwrap()
    }

    fn maps(job: &Job) -> Vec<String> {
        job.args
            .windows(2)
            .filter(|w| w[0] == "-map")
            .map(|w| w[1].clone())
            .collect()
    }

    fn filter_complex(job: &Job) -> String {
        job.args
            .iter()
            .position(|a| a == "-filter_complex")
            .map(|i| job.args[i + 1].clone())
            .unwrap_or_default()
    }

    #[test]
    fn output_path_tokens() {
        let cfg = PackageConfig::default();
        let input = Path::new("/videos/a/clip.mp4");
        let out = render_output_path(&cfg, input, 3, Some(Path::new("/videos")));
        assert_eq!(out, PathBuf::from("/videos/a/clip_fflow.mp4"));
    }

    // -- several terminal Packages -----------------------------------------

    fn node(id: u64, kind: NodeKind, cfg: serde_json::Value) -> crate::graph::model::WfNode {
        crate::graph::model::WfNode {
            id,
            kind: kind.id().into(),
            config: cfg,
            pos: None,
        }
    }

    fn edge(from: (u64, &str), to: (u64, &str)) -> crate::graph::model::Edge {
        crate::graph::model::Edge {
            from: (from.0, from.1.into()),
            to: (to.0, to.1.into()),
        }
    }

    fn condition(rules: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "label": "", "match_all": true, "rules": rules })
    }

    fn demux_cfg() -> serde_json::Value {
        serde_json::json!({
            "audio_mode": "First", "audio_tracks": [],
            "subtitle_mode": "Off", "subtitle_tracks": [],
            "video_mode": "First", "video_tracks": []
        })
    }

    fn package_cfg(container: &str) -> serde_json::Value {
        serde_json::json!({
            "container": container, "extra_args": "", "metadata": [],
            "movflags_faststart": true, "name_template": "{dir}/{stem}_fflow.{ext}",
            "output_dir": "", "shortest": false
        })
    }

    /// The workflow from the bug report: one Source feeding two branches, each
    /// ending in its own Package (mp4 for the video side, wav for the audio
    /// side). Both branches are gated by a Condition *in front of* the Demux.
    fn two_branch_workflow(gate_a: serde_json::Value, gate_b: serde_json::Value) -> crate::graph::Workflow {
        crate::graph::Workflow {
            version: "1.0".into(),
            name: "two outputs".into(),
            nodes: vec![
                node(
                    0,
                    NodeKind::Source,
                    serde_json::json!({
                        "extensions": "mp4,mkv", "paths": ["/tmp"], "recursive": true,
                        "skip_own_outputs": true
                    }),
                ),
                node(1, NodeKind::Condition, gate_a),
                node(2, NodeKind::Demux, demux_cfg()),
                node(
                    3,
                    NodeKind::VideoEncoder,
                    serde_json::json!({
                        "bitrate": "4M", "codec": "libx264", "crf": 23,
                        "pix_fmt": "yuv420p", "prefer_hardware": false,
                        "preset": "medium", "use_bitrate": false
                    }),
                ),
                node(4, NodeKind::Package, package_cfg("mp4")),
                node(5, NodeKind::Condition, gate_b),
                node(6, NodeKind::Demux, demux_cfg()),
                node(
                    7,
                    NodeKind::AudioEncoder,
                    serde_json::json!({
                        "bitrate": "192k", "channels": 2, "codec": "aac",
                        "sample_rate": 48000
                    }),
                ),
                node(8, NodeKind::Package, package_cfg("wav")),
            ],
            edges: vec![
                edge((0, "file"), (1, "in")),
                edge((1, "pass"), (2, "in")),
                edge((2, "video"), (3, "in")),
                edge((3, "out"), (4, "video")),
                edge((0, "file"), (5, "in")),
                edge((5, "pass"), (6, "in")),
                edge((6, "audio"), (7, "in")),
                edge((7, "out"), (8, "audio")),
            ],
        }
    }

    fn mp4_gate() -> serde_json::Value {
        condition(serde_json::json!([
            { "field": "Extension", "key": "", "op": "Equals", "value": "mp4" }
        ]))
    }

    /// Two terminal Packages must each produce a job — this is the case the
    /// compiler used to reject with "a workflow needs exactly one final
    /// Package".
    #[test]
    fn two_terminal_packages_yield_one_job_each() {
        // Both gates accept an mp4/h264 file, so both branches are live.
        let wf = two_branch_workflow(mp4_gate(), mp4_gate());
        let editor = editor_from(wf);
        let jobs = compile_all(
            &editor.graph,
            Path::new("/tmp/clip.mp4"),
            &probe(1, 1, 0),
            1,
            None,
        )
        .expect("both branches compile");

        assert_eq!(jobs.len(), 2, "one job per terminal Package");
        let mut exts: Vec<String> = jobs
            .iter()
            .map(|j| {
                j.output
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect();
        exts.sort();
        assert_eq!(exts, vec!["mp4", "wav"]);
    }

    /// A Condition in front of the Demux routes whole files: the file only
    /// takes the branch whose gate passes.
    #[test]
    fn a_condition_before_the_demux_gates_the_branch() {
        let mkv = condition(serde_json::json!([
            { "field": "Extension", "key": "", "op": "Equals", "value": "mkv" }
        ]));
        let wf = two_branch_workflow(mp4_gate(), mkv);
        let editor = editor_from(wf);

        // clip.mp4 takes branch A and is turned away from branch B.
        let jobs = compile_all(
            &editor.graph,
            Path::new("/tmp/clip.mp4"),
            &probe(1, 1, 0),
            1,
            None,
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].output.ends_with("clip_fflow.mp4"));

        // A webm file matches neither gate: skipped, not failed.
        let err = compile_all(
            &editor.graph,
            Path::new("/tmp/clip.webm"),
            &probe(1, 1, 0),
            1,
            None,
        )
        .expect_err("no branch takes a webm file");
        assert!(
            skipped_reason(&err).is_some(),
            "expected a skip, got {:#}",
            err
        );
    }

    /// The preview must agree with the compiler: same branch, same nodes, and
    /// a gate in front of the Demux is part of the path, not a bystander.
    #[test]
    fn trace_lights_up_the_branch_that_takes_the_file() {
        let mkv = condition(serde_json::json!([
            { "field": "Extension", "key": "", "op": "Equals", "value": "mkv" }
        ]));
        let wf = two_branch_workflow(mp4_gate(), mkv);
        let editor = editor_from(wf);
        let t = trace(&editor.graph, Path::new("/tmp/clip.mp4"), &probe(1, 1, 0));

        let id = |json_id: u64| {
            crate::graph::node_by_json_id(&editor.graph, json_id)
                .unwrap_or_else(|| panic!("no node with json id {json_id}"))
        };
        // Branch A: Source 0 → Condition 1 → Demux 2 → Encoder 3 → Package 4.
        for json_id in [0, 1, 2, 3, 4] {
            assert!(
                t.nodes.contains(&id(json_id)),
                "node {json_id} is on the live branch and must be highlighted"
            );
        }
        // Branch B is a different file's job, so it stays dim.
        for json_id in [5, 6, 7, 8] {
            assert!(
                !t.nodes.contains(&id(json_id)),
                "node {json_id} is on the other branch and must stay dim"
            );
        }

        // Every wire the file crosses is lit, including the gate's own edges.
        assert_eq!(t.links.len(), 4, "Source→Cond, Cond→Demux, Demux→Enc, Enc→Pkg");

        assert!(t.flows());
        assert_eq!(t.taken().count(), 1, "only the mp4 Package takes it");
        let taken = t.taken().next().unwrap();
        assert_eq!(taken.target, "mp4");
        assert!(taken.path.len() >= 5, "path: {:?}", taken.path);
        assert_eq!(taken.path.first().unwrap(), "Source");
        assert_eq!(taken.path.last().unwrap(), "Package");

        // The wav output says why it said no.
        let skipped = t.outputs.iter().find(|o| !o.taken).expect("one refusal");
        assert_eq!(skipped.target, "wav");
        assert!(
            skipped.reason.as_deref().unwrap_or("").contains("condition"),
            "reason: {:?}",
            skipped.reason
        );
    }

    /// A file that no branch wants is not an error in the preview: it is the
    /// answer "nothing will be written", with the gate that turned it away.
    #[test]
    fn trace_reports_a_file_that_flows_nowhere() {
        let wf = two_branch_workflow(mp4_gate(), mp4_gate());
        let editor = editor_from(wf);
        let t = trace(&editor.graph, Path::new("/tmp/clip.webm"), &probe(1, 1, 0));

        assert!(!t.flows());
        assert!(t.nodes.is_empty(), "nothing is highlighted: {:?}", t.nodes);
        assert!(t.links.is_empty());
        assert_eq!(t.outputs.len(), 2, "one verdict per Package");
        assert!(t.outputs.iter().all(|o| o.reason.is_some()));
    }

    /// Two Packages writing the same path is a mistake worth reporting: ffmpeg
    /// would silently keep whichever finished last.
    #[test]
    fn duplicate_output_paths_are_rejected() {
        let wf = two_branch_workflow(mp4_gate(), mp4_gate());
        let mut editor = editor_from(wf);
        // Make branch B write mp4 as well.
        for (_, n) in editor.graph.nodes.iter_mut() {
            if n.user_data.json_id == 8 {
                n.user_data.config = NodeConfig::Package(PackageConfig {
                    container: "mp4".into(),
                    ..PackageConfig::default()
                });
            }
        }
        let err = compile_all(
            &editor.graph,
            Path::new("/tmp/clip.mp4"),
            &probe(1, 1, 0),
            1,
            None,
        )
        .expect_err("both packages would write clip_fflow.mp4");
        assert!(
            format!("{:#}", err).contains("both write"),
            "unexpected error: {:#}",
            err
        );
    }

    #[test]
    fn overlay_builds_valid_filtergraph() {
        use crate::graph::model::{Edge, WfNode};
        use crate::graph::nodes::OverlayConfig;

        let mut wf = crate::graph::default_workflow(None);
        wf.nodes.push(WfNode {
            id: 9,
            kind: NodeKind::Overlay.id().into(),
            config: serde_json::to_value(OverlayConfig {
                path: "/tmp/logo.png".into(),
                x: "10".into(),
                y: "10".into(),
                scale: 1.0,
            })
            .unwrap(),
            pos: None,
        });
        // Splice the card between Demux.video and the video encoder.
        wf.edges.retain(|e| !(e.from.0 == 1 && e.from.1 == "video"));
        wf.edges.push(Edge {
            from: (1, "video".into()),
            to: (9, "in".into()),
        });
        wf.edges.push(Edge {
            from: (9, "out".into()),
            to: (2, "in".into()),
        });

        let editor = editor_from(wf);
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 1, 0), 1, None).unwrap();
        let fc = filter_complex(&job);
        assert_eq!(fc, "[0:v][1:v]overlay=10:10:shortest=1[v]");
        assert!(job.args.iter().any(|a| a == "/tmp/logo.png"));
        // the image input must come after the source input
        let first_i = job.args.iter().position(|a| a == "-i").unwrap();
        let second_i = job.args.iter().skip(first_i + 1).position(|a| a == "-i").unwrap() + first_i + 1;
        assert_eq!(job.args[second_i + 1], "/tmp/logo.png");
    }

    #[test]
    fn output_path_index() {
        let mut cfg = PackageConfig::default();
        cfg.name_template = "{dir}/{index}_{stem}.{ext}".into();
        let input = Path::new("/v/a.mp4");
        let out = render_output_path(&cfg, input, 7, None);
        assert_eq!(out, PathBuf::from("/v/007_a.mp4"));
    }

    #[test]
    fn labels_follow_track_ordinals() {
        assert_eq!(source_label(true, 0), "[0:v]");
        assert_eq!(source_label(true, 2), "[0:v:2]");
        assert_eq!(source_label(false, 1), "[0:a:1]");
        assert_eq!(output_label(true, 0, 1), "[v]");
        assert_eq!(output_label(true, 1, 2), "[v1]");
        assert_eq!(output_label(false, 0, 3), "[a0]");
        assert_eq!(map_target(false, 2), "0:a:2");
    }

    #[test]
    fn single_track_keeps_terse_labels() {
        let editor = editor_with_demux(DemuxConfig::default());
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 1, 0), 1, None).unwrap();
        assert_eq!(filter_complex(&job), "[0:v]scale=1280:-2[v]");
        assert_eq!(maps(&job), vec!["[v]", "0:a:0"]);
        assert_eq!(job.args.iter().filter(|a| *a == "-c:v").count(), 1);
        assert_eq!(job.args.iter().filter(|a| *a == "-c:a").count(), 1);
    }

    #[test]
    fn maps_every_audio_track() {
        let editor = editor_from(workflow_with(
            DemuxConfig {
                audio_mode: TrackMode::All,
                ..Default::default()
            },
            true,
        ));
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 3, 0), 1, None).unwrap();
        println!("{}", job.display);
        assert_eq!(maps(&job), vec!["[v]", "[a0]", "[a1]", "[a2]"]);
        let fc = filter_complex(&job);
        // each branch must read its own source stream
        assert!(fc.contains("[0:a]volume=3dB[a0]"), "{}", fc);
        assert!(fc.contains("[0:a:1]volume=3dB[a1]"), "{}", fc);
        assert!(fc.contains("[0:a:2]volume=3dB[a2]"), "{}", fc);
        // `-c:a` / `-c:v` are emitted exactly once even with several tracks
        assert_eq!(job.args.iter().filter(|a| *a == "-c:a").count(), 1);
        assert_eq!(job.args.iter().filter(|a| *a == "-c:v").count(), 1);
    }

    #[test]
    fn selects_specific_tracks_only() {
        let editor = editor_from(workflow_with(
            DemuxConfig {
                audio_mode: TrackMode::Select,
                audio_tracks: vec![1],
                ..Default::default()
            },
            true,
        ));
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 3, 0), 1, None).unwrap();
        assert_eq!(maps(&job), vec!["[v]", "[a]"]);
        let fc = filter_complex(&job);
        assert!(fc.contains("[0:a:1]"), "picked track 1, got {}", fc);
        assert!(!fc.contains("[0:a]volume"), "track 0 must not be read: {}", fc);
    }

    #[test]
    fn out_of_range_tracks_are_dropped() {
        let editor = editor_with_demux(DemuxConfig {
            audio_mode: TrackMode::Select,
            audio_tracks: vec![0, 5],
            ..Default::default()
        });
        // the file only has one audio track, ordinal 5 does not exist
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 1, 0), 1, None).unwrap();
        assert_eq!(maps(&job), vec!["[v]", "0:a:0"]);
    }

    #[test]
    fn audio_branch_off_drops_it() {
        let editor = editor_with_demux(DemuxConfig {
            audio_mode: TrackMode::Off,
            ..Default::default()
        });
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 2, 0), 1, None).unwrap();
        assert_eq!(maps(&job), vec!["[v]"]);
        assert_eq!(job.args.iter().filter(|a| *a == "-c:a").count(), 0);
    }

    #[test]
    fn subtitles_are_mapped_with_mov_text() {
        let d = DemuxConfig {
            subtitle_mode: TrackMode::All,
            ..Default::default()
        };
        let editor = editor_with_demux(d);
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(1, 1, 2), 1, None).unwrap();
        assert!(job.args.iter().any(|a| a == "0:s:0"), "{}", job.display);
        assert!(job.args.iter().any(|a| a == "0:s:1"));
        let idx = job.args.iter().position(|a| a == "-c:s").unwrap();
        assert_eq!(job.args[idx + 1], "mov_text");
    }

    #[test]
    fn subtitles_are_dropped_for_audio_containers() {
        let mut editor = editor_with_demux(DemuxConfig {
            subtitle_mode: TrackMode::All,
            ..Default::default()
        });
        let pkg = crate::graph::package_nodes(&editor.graph)[0];
        if let NodeConfig::Package(c) = &mut editor.graph.nodes[pkg].user_data.config {
            c.container = "mp3".into();
        }
        let job = compile(&editor.graph, Path::new("in.mp4"), &probe(0, 1, 2), 1, None).unwrap();
        assert!(!job.args.iter().any(|a| a == "-c:s"));
        assert!(job.notes.iter().any(|n| n.contains("cannot carry subtitles")));
    }

    #[test]
    fn hardware_codec_swaps_quality_flags() {
        // No registry installed in tests → resolve() is the identity, so we
        // exercise args_for directly with a hardware name.
        let c = VideoEncoderConfig {
            crf: 24,
            preset: "medium".into(),
            ..Default::default()
        };
        let nvenc = c.args_for("h264_nvenc");
        assert!(nvenc.contains(&"-cq".to_string()));
        assert!(nvenc.contains(&"24".to_string()));
        assert!(!nvenc.contains(&"-crf".to_string()));
        let qsv = c.args_for("h264_qsv");
        assert!(qsv.contains(&"-global_quality".to_string()));
        let amf = c.args_for("hevc_amf");
        assert!(amf.contains(&"-qp_i".to_string()));
        assert!(amf.contains(&"-quality".to_string()));
        // software keeps -crf + -preset
        let sw = c.args_for("libx264");
        assert!(sw.contains(&"-crf".to_string()));
        assert!(sw.contains(&"-preset".to_string()));
    }
    /// Video branch:
    ///
    /// ```text
    /// Demux.video ─▶ Condition.in ─┬─ pass ─▶ Scale 1920 ─┐
    ///                              └─ fail ─▶ Scale 640  ─┴─▶ Video Encoder ─▶ Package
    /// ```
    fn branching_workflow(connect_fail: bool) -> crate::graph::Workflow {
        use crate::graph::model::{Edge, WfNode};
        use crate::graph::nodes::{CondField, CondOp, CondRule, ConditionConfig};

        let mut wf = crate::graph::default_workflow(None);
        wf.nodes.push(WfNode {
            id: 5,
            kind: NodeKind::Condition.id().into(),
            config: serde_json::to_value(ConditionConfig {
                rules: vec![CondRule {
                    field: CondField::Extension,
                    op: CondOp::Equals,
                    value: "mp4".into(),
                    key: String::new(),
                }],
                match_all: true,
                label: "is mp4".into(),
            })
            .unwrap(),
            pos: None,
        });
        for (id, w, h) in [(6u64, "1920", "1080"), (7, "640", "360")] {
            wf.nodes.push(WfNode {
                id,
                kind: NodeKind::Scale.id().into(),
                config: serde_json::to_value(crate::graph::nodes::ScaleConfig {
                    width: w.into(),
                    height: h.into(),
                    mode: crate::graph::nodes::ScaleMode::Exact,
                })
                .unwrap(),
                pos: None,
            });
        }

        // Demux now feeds the Condition, and both sides re-join at the encoder
        // (its input accepts several connections). Only the Demux → encoder
        // edge goes away; the encoder must keep feeding the Package.
        wf.edges.retain(|e| !(e.from.0 == 1 && e.from.1 == "video"));
        wf.edges.push(Edge {
            from: (1, "video".into()),
            to: (5, "in".into()),
        });
        wf.edges.push(Edge {
            from: (5, "pass".into()),
            to: (6, "in".into()),
        });
        wf.edges.push(Edge {
            from: (6, "out".into()),
            to: (2, "in".into()),
        });
        if connect_fail {
            wf.edges.push(Edge {
                from: (5, "fail".into()),
                to: (7, "in".into()),
            });
            wf.edges.push(Edge {
                from: (7, "out".into()),
                to: (2, "in".into()),
            });
        }
        wf
    }

    #[test]
    fn package_can_feed_a_second_stage() {
        use crate::graph::model::{Edge, WfNode};
        use crate::graph::nodes::TrackMode;

        // Stage 1 is the default pipeline (Source → Demux → encoders →
        // Package 4). Its `file` output feeds a second Demux, which means
        // "write the file, then keep processing it".
        let mut wf = crate::graph::default_workflow(None);
        wf.nodes.push(WfNode {
            id: 5,
            kind: NodeKind::Demux.id().into(),
            config: serde_json::to_value(DemuxConfig {
                audio_mode: TrackMode::Off,
                ..Default::default()
            })
            .unwrap(),
            pos: None,
        });
        wf.nodes.push(WfNode {
            id: 6,
            kind: NodeKind::VideoEncoder.id().into(),
            config: serde_json::json!({}),
            pos: None,
        });
        wf.nodes.push(WfNode {
            id: 7,
            kind: NodeKind::Package.id().into(),
            config: serde_json::json!({
                "container": "mkv",
                "name_template": "{dir}/{stem}_final.{ext}"
            }),
            pos: None,
        });
        wf.edges.push(Edge {
            from: (4, "file".into()),
            to: (5, "in".into()),
        });
        wf.edges.push(Edge {
            from: (5, "video".into()),
            to: (6, "in".into()),
        });
        wf.edges.push(Edge {
            from: (6, "out".into()),
            to: (7, "video".into()),
        });

        let editor = editor_from(wf);
        let job = compile(&editor.graph, Path::new("/v/clip.mp4"), &probe(1, 1, 0), 1, None)
            .expect("two-stage workflow compiles");

        assert_eq!(job.stages.len(), 2, "one ffmpeg run per Package");
        assert_eq!(job.output, PathBuf::from("/v/clip_final.mkv"));
        assert_eq!(job.args, job.stages[1].args, "`args` mirrors the last stage");
        assert!(job.display.starts_with("[1] "), "{}", job.display);
        assert!(job.display.contains("\n[2] "), "{}", job.display);

        // Stage 2 reads what stage 1 wrote, not the source file.
        let mid = &job.stages[0].output;
        assert_ne!(mid, &job.output);
        assert!(
            mid.to_string_lossy().contains("fflow"),
            "intermediate goes to the temp dir: {}",
            mid.display()
        );
        let i = job.stages[1].args.iter().position(|a| a == "-i").unwrap();
        assert_eq!(&job.stages[1].args[i + 1], &mid.to_string_lossy().to_string());
        assert!(job
            .notes
            .iter()
            .any(|n| n.contains("stage 1 writes")), "{:?}", job.notes);
    }

    #[test]
    fn condition_picks_one_branch_per_file() {
        let editor = editor_from(branching_workflow(true));
        let mp4 = compile(&editor.graph, Path::new("clip.mp4"), &probe(1, 1, 0), 1, None).unwrap();
        assert!(filter_complex(&mp4).contains("scale=1920:1080"), "{}", mp4.display);
        assert!(!filter_complex(&mp4).contains("scale=640:360"));

        let mov = compile(&editor.graph, Path::new("clip.mov"), &probe(1, 1, 0), 1, None).unwrap();
        assert!(filter_complex(&mov).contains("scale=640:360"), "{}", mov.display);
        assert!(!filter_complex(&mov).contains("scale=1920:1080"));
    }

    #[test]
    fn condition_without_else_skips_non_matching_files() {
        // Video only: the condition gates the whole job.
        let mut wf = branching_workflow(false);
        for n in &mut wf.nodes {
            if n.kind == NodeKind::Demux.id() {
                n.config = serde_json::to_value(DemuxConfig {
                    audio_mode: TrackMode::Off,
                    ..Default::default()
                })
                .unwrap();
            }
        }
        let editor = editor_from(wf);
        // mp4 matches → then branch, normal job
        assert!(compile(&editor.graph, Path::new("clip.mp4"), &probe(1, 1, 0), 1, None).is_ok());
        // mov does not match and `else` is unconnected → skipped, not an error
        let err = compile(&editor.graph, Path::new("clip.mov"), &probe(1, 1, 0), 1, None).unwrap_err();
        assert!(skipped_reason(&err).is_some(), "{:#}", err);
    }

    #[test]
    fn dropped_branch_keeps_the_other_stream() {
        // Audio is not gated by the condition, so it still gets encoded.
        let editor = editor_from(branching_workflow(false));
        let job = compile(&editor.graph, Path::new("clip.mov"), &probe(1, 1, 0), 1, None).unwrap();
        assert_eq!(maps(&job), vec!["0:a:0"]);
        assert!(job.notes.iter().any(|n| n.contains("condition")), "{:?}", job.notes);
    }
}
