//! fflow — a Scratch-like visual front-end for batch ffmpeg processing.
//!
//! Pure Rust desktop app (eframe/egui). Media work is always delegated to the
//! system `ffmpeg` / `ffprobe` executables; no libav bindings are linked.

mod app;
mod compiler;
mod encoders;
mod graph;
mod i18n;
mod probe;
mod runner;
mod shot;
mod source;
mod theme;

use std::path::{Path, PathBuf};

use eframe::egui;

/// `--flag value` lookup.
fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fflow=info".into()),
        )
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--lang` is applied through the environment so the GUI, the offscreen
    // renderer and `--selftest` all take the same path. A saved settings file
    // still wins on a normal launch; this is a per-run override.
    if let Some(code) = arg_value(&args, "--lang") {
        match i18n::Language::from_code(&code) {
            Some(language) => std::env::set_var("FFLOW_LANG", language.code()),
            None => eprintln!("unknown language `{code}` (expected en or zh)"),
        }
    }
    if args.first().map(|a| a == "--selftest").unwrap_or(false) {
        let dir = args.get(1).map(PathBuf::from);
        let wf = arg_value(&args, "--workflow").map(PathBuf::from);
        let code = match selftest(dir, wf) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("selftest failed: {:#}", e);
                1
            }
        };
        std::process::exit(code);
    }

    // Offscreen render of the UI (no window, no GPU, no display needed).
    if let Some(out) = arg_value(&args, "--screenshot") {
        let (width, height) = arg_value(&args, "--size")
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((1600usize, 980usize));
        let opts = shot::Options {
            out: PathBuf::from(out),
            width,
            height,
            select: arg_value(&args, "--select").and_then(|s| s.parse().ok()),
            dir: arg_value(&args, "--dir").map(PathBuf::from),
            probe: arg_value(&args, "--probe").map(PathBuf::from),
            detect: !args.iter().any(|a| a == "--no-detect"),
            demo_tasks: args.iter().any(|a| a == "--demo-tasks"),
            show_help: args.iter().any(|a| a == "--show-help"),
            light: args.iter().any(|a| a == "--light"),
            workflow: arg_value(&args, "--workflow").map(PathBuf::from),
            zoom: arg_value(&args, "--zoom").and_then(|s| s.parse().ok()),
            // `--flow` takes an optional file index, so it is parsed here
            // rather than with `arg_value` (which would eat the next flag).
            flow: args.iter().position(|a| a == "--flow").map(|i| {
                args.get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0usize)
            }),
        };
        std::process::exit(match shot::render(&opts) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("screenshot failed: {:#}", e);
                1
            }
        });
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "fflow {}\n\n  fflow                       launch the GUI\n  \
             fflow --selftest <dir>      headless end-to-end check on <dir>\n  \
             fflow --screenshot <out.ppm> [--size 1600x980] [--select N]\n                              \
             [--dir <folder>] [--probe <file>] [--workflow <file>] [--zoom 1.5]\n                              \
             [--no-detect] [--demo-tasks] [--show-help] [--light]\n                              \
             [--flow [N]] [--lang en|zh]\n                              \
             render the UI offscreen (no display required)\n\n\
             Global: --lang en|zh switches the UI language for this run.\n\
             Environment: FFLOW_FFMPEG, FFLOW_FFPROBE override the binaries;\n             \
             FFLOW_LANG does the same as --lang.",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([980.0, 620.0])
            .with_title("fflow — visual ffmpeg batch processor")
            .with_icon(app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "fflow",
        options,
        Box::new(|cc| Ok(Box::new(app::FflowApp::new(cc)) as Box<dyn eframe::App>)),
    )
}

/// The window icon, decoded from the PNG that `tools/make_icon.py` writes.
///
/// `image` is already in the dependency tree — eframe decodes PNGs with it for
/// its own tooling — so this costs no extra crate. 128 px covers a 32 px
/// taskbar button up to 400 % scaling and still downscales cleanly to the
/// 16 px title bar.
///
/// A decode failure is logged and ignored: an icon-less window is a cosmetic
/// problem, refusing to start is not.
fn app_icon() -> egui::IconData {
    const PNG: &[u8] = include_bytes!("../assets/icon-128.png");
    eframe::icon_data::from_png_bytes(PNG).unwrap_or_else(|e| {
        tracing::warn!("window icon could not be decoded: {e}");
        egui::IconData::default()
    })
}

/// The asset is generated (`tools/make_icon.py`), so it can be regenerated in a
/// format this build cannot read. `app_icon` deliberately degrades to no icon
/// rather than aborting, which would hide that — this test is what notices.
#[test]
fn the_window_icon_decodes() {
    let icon = app_icon();
    assert_eq!((icon.width, icon.height), (128, 128));
    assert_eq!(icon.rgba.len(), 128 * 128 * 4);
    // Fully opaque in the middle of the tile, transparent in the corner the
    // rounded tile cuts away: a blank or one-colour decode would fail here.
    let centre = (64 * 128 + 64) * 4;
    let corner = 0;
    assert_eq!(icon.rgba[centre + 3], 255, "the tile should be opaque");
    assert_eq!(icon.rgba[corner + 3], 0, "the tile corner should be cut away");
}

const STEPS: usize = 7;

fn step(n: usize, msg: impl std::fmt::Display) {
    println!("   [{}/{}] {}", n, STEPS, msg);
}

/// Drive a runner to completion, printing progress buckets and results.
fn run_to_completion(runner: &mut runner::Runner, tag: usize) -> anyhow::Result<(usize, usize)> {
    let mut seen: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
    let (mut ok, mut failed) = (0usize, 0usize);
    loop {
        for ev in runner.poll() {
            match ev {
                runner::RunEvent::Progress { id, pct, .. } => {
                    let bucket = (pct / 25.0) as i32;
                    if seen.insert(id, bucket) != Some(bucket) {
                        step(tag, format!("job {} → {:.0}%", id, pct));
                    }
                }
                runner::RunEvent::Finished {
                    id,
                    ok: job_ok,
                    cancelled,
                    error,
                    secs,
                    output,
                } => {
                    if job_ok {
                        ok += 1;
                    } else {
                        failed += 1;
                    }
                    step(
                        tag,
                        format!(
                            "job {} {} in {:.2}s → {}",
                            id,
                            if job_ok {
                                "OK"
                            } else if cancelled {
                                "CANCELLED"
                            } else {
                                "FAILED"
                            },
                            secs,
                            output.display()
                        ),
                    );
                    if let Some(e) = error {
                        println!("          {}", e.lines().last().unwrap_or(""));
                    }
                }
                runner::RunEvent::BatchDone {
                    ok: o,
                    failed: f,
                    cancelled,
                } => {
                    step(tag, format!("batch: {} ok, {} failed, {} cancelled", o, f, cancelled));
                }
                _ => {}
            }
        }
        if !runner.is_running() {
            return Ok((ok, failed));
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

/// Headless end-to-end verification used during development:
/// detect encoders, build the default pipeline, probe every file, compile, run
/// the batch, cancel a job mid-flight, and exercise multi-track mapping.
fn selftest(dir: Option<PathBuf>, workflow: Option<PathBuf>) -> anyhow::Result<()> {
    use anyhow::Context;

    let dir = dir
        .or_else(|| std::env::current_dir().ok())
        .context("no directory given")?;
    println!("== fflow selftest on {}", dir.display());

    // 1. workflow JSON round-trip
    let wf = match &workflow {
        Some(path) => graph::Workflow::from_json(&std::fs::read_to_string(path)?)?,
        None => graph::default_workflow(Some(&dir)),
    };
    let json = wf.to_json()?;
    let wf2 = graph::Workflow::from_json(&json)?;
    let mut ctx = graph::GraphCtx::default();
    let editor = graph::from_workflow(&wf2, &mut ctx)?;
    let wf3 = graph::to_workflow(&editor);
    anyhow::ensure!(
        wf3.nodes.len() == wf.nodes.len() && wf3.edges.len() == wf.edges.len(),
        "workflow round-trip lost nodes/edges ({} nodes, {} edges)",
        wf3.nodes.len(),
        wf3.edges.len()
    );
    step(
        1,
        format!(
            "workflow JSON round-trip ok ({} nodes, {} edges)",
            wf3.nodes.len(),
            wf3.edges.len()
        ),
    );

    // 2. encoder detection (list + real one-frame hardware probes)
    let reg = encoders::detect_now();
    let hw = reg.working_hardware();
    step(
        2,
        format!(
            "encoders: {} video / {} audio · hardware usable: {}",
            reg.video.len(),
            reg.audio.len(),
            if hw.is_empty() {
                "none".to_string()
            } else {
                hw.join(", ")
            }
        ),
    );
    if let Some(err) = &reg.error {
        println!("          ⚠ {}", err);
    }

    // 3. enumerate + probe
    let files = source::expand(
        &[dir.to_string_lossy().to_string()],
        true,
        graph::nodes::DEFAULT_EXTENSIONS,
        true,
    );
    anyhow::ensure!(!files.is_empty(), "no media files found in {}", dir.display());
    let rt = tokio::runtime::Runtime::new()?;
    let mut probes = std::collections::HashMap::new();
    for f in &files {
        let info = rt.block_on(probe::probe(f))?;
        step(
            3,
            format!(
                "{} — {} · {} · {:.2}s",
                source::short_name(f),
                info.streams_summary(),
                info.resolution(),
                info.duration
            ),
        );
        probes.insert(f.to_string_lossy().to_string(), info);
    }

    // 4. compile. A Condition may legitimately send a file down no branch at
    // all, so a skip is counted rather than treated as a failure.
    let mut jobs: Vec<compiler::Job> = Vec::new();
    let mut skipped = 0usize;
    for result in compiler::compile_batch(&editor.graph, &files, &probes, Some(&dir)) {
        match result {
            Ok(batch) => jobs.extend(batch),
            Err(e) if compiler::skipped_reason(&e).is_some() => skipped += 1,
            Err(e) => return Err(e).context("compile failed"),
        }
    }
    if skipped > 0 {
        step(4, format!("{} file(s) skipped by a condition", skipped));
    }
    for job in &jobs {
        step(4, &job.display);
        for n in &job.notes {
            println!("          · {}", n);
        }
    }

    // 5. run the batch
    let mut runner = runner::Runner::new()?;
    runner.start(jobs.clone(), 2);
    let (ok, failed) = run_to_completion(&mut runner, 5)?;
    anyhow::ensure!(failed == 0, "{} job(s) failed", failed);
    anyhow::ensure!(ok == jobs.len(), "only {} of {} jobs succeeded", ok, jobs.len());

    let mut missing = Vec::new();
    for job in &jobs {
        if !job.output.exists() {
            missing.push(job.output.clone());
        }
    }
    anyhow::ensure!(missing.is_empty(), "missing outputs: {:?}", missing);
    step(5, format!("{} output file(s) written", jobs.len()));

    // 6. cancellation: run one job at native frame rate (`-re`) so it takes
    //    seconds, then cancel and make sure the child really died.
    let mut slow = jobs[0].clone();
    slow.id = 99;
    slow.output = slow.output.with_file_name("cancelled_fflow.mp4");
    // The runner executes `stages`, not `args` — retarget the last stage and
    // keep the convenience fields in sync with it.
    slow.args.clear();
    if let Some(stage) = slow.stages.last_mut() {
        stage.output = slow.output.clone();
        // `-re` is an input option and throttles reading to native frame rate,
        // so the job takes seconds instead of milliseconds — deterministic to
        // cancel.
        if let Some(pos) = stage.args.iter().position(|a| a == "-i") {
            stage.args.insert(pos, "-re".to_string());
        }
        if let Some(last) = stage.args.last_mut() {
            *last = slow.output.to_string_lossy().to_string();
        }
        slow.args = stage.args.clone();
    }
    let mut runner = runner::Runner::new()?;
    runner.start(vec![slow.clone()], 1);
    std::thread::sleep(std::time::Duration::from_millis(1400));
    runner.cancel_all();

    let mut ticks = 0;
    let mut cancelled_ok = false;
    let started = std::time::Instant::now();
    loop {
        for ev in runner.poll() {
            match ev {
                runner::RunEvent::Progress { .. } => ticks += 1,
                runner::RunEvent::Finished { cancelled, ok, .. } => {
                    cancelled_ok = cancelled && !ok;
                }
                _ => {}
            }
        }
        if !runner.is_running() {
            break;
        }
        anyhow::ensure!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "cancel did not stop the batch"
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    anyhow::ensure!(cancelled_ok, "cancel was not reported for the running job");
    anyhow::ensure!(
        !slow.output.exists(),
        "partial output was left behind: {}",
        slow.output.display()
    );
    step(
        6,
        format!("cancel ok ({} progress ticks, child killed, no partial file)", ticks),
    );

    // 7. multi-track mapping: build a 1-video + 2-audio fixture, ask the Demux
    //    node for *all* audio tracks and check both survive the round trip.
    multi_track_check(&files, &probes, &rt)?;

    println!("== selftest passed");
    Ok(())
}

fn multi_track_check(
    files: &[PathBuf],
    probes: &std::collections::HashMap<String, probe::ProbeInfo>,
    rt: &tokio::runtime::Runtime,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use graph::nodes::{NodeConfig, NodeKind, TrackMode};

    let tmp = std::env::temp_dir().join("fflow_selftest");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;

    let src: &Path = files
        .iter()
        .find(|f| {
            probes
                .get(&f.to_string_lossy().to_string())
                .map(|p| p.has_video && p.has_audio)
                .unwrap_or(false)
        })
        .map(|p| p.as_path())
        .context("multi-track check needs one file with both video and audio")?;

    // Muxing the same file twice yields 1 video + 2 audio tracks.
    let multi = tmp.join("multi.mp4");
    let status = std::process::Command::new(probe::ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .arg("-i")
        .arg(src)
        .args([
            "-map", "0:v:0", "-map", "0:a:0", "-map", "1:a:0", "-c", "copy",
        ])
        .arg(&multi)
        .status()?;
    anyhow::ensure!(status.success(), "could not build the multi-track fixture");

    let info = rt.block_on(probe::probe(&multi))?;
    anyhow::ensure!(
        info.audio_streams.len() == 2 && info.video_streams.len() == 1,
        "fixture should have 1 video + 2 audio, got {} + {}",
        info.video_streams.len(),
        info.audio_streams.len()
    );

    let mut ctx = graph::GraphCtx::default();
    let mut editor = graph::from_workflow(&graph::default_workflow(Some(&tmp)), &mut ctx)?;
    for id in editor.graph.iter_nodes().collect::<Vec<_>>() {
        if let NodeConfig::Demux(c) = &mut editor.graph.nodes[id].user_data.config {
            c.audio_mode = TrackMode::All;
        }
    }
    let _ = NodeKind::Demux;

    let job = compiler::compile(&editor.graph, &multi, &info, 1, Some(&tmp))?;
    step(7, &job.display);

    let mut runner = runner::Runner::new()?;
    runner.start(vec![job.clone()], 1);
    let (ok, failed) = run_to_completion(&mut runner, 7)?;
    anyhow::ensure!(failed == 0 && ok == 1, "multi-track job failed");

    let out = rt.block_on(probe::probe(&job.output))?;
    anyhow::ensure!(
        out.audio_streams.len() == 2,
        "expected 2 audio tracks in the output, found {}",
        out.audio_streams.len()
    );
    anyhow::ensure!(
        out.video_streams.len() == 1,
        "expected 1 video track in the output, found {}",
        out.video_streams.len()
    );
    let langs: Vec<String> = out.audio_streams.iter().map(|s| s.codec.clone()).collect();
    step(
        7,
        format!(
            "multi-track ok (1 video + {} audio: {})",
            out.audio_streams.len(),
            langs.join(", ")
        ),
    );

    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}
