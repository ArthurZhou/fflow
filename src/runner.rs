//! Batch execution engine.
//!
//! * every ffmpeg invocation is a child process (`tokio::process`)
//! * concurrency is capped by a `Semaphore`
//! * `-progress pipe:1` output drives the per-file progress bars
//! * every job has its own `CancellationToken`; cancelling kills the child and
//!   removes the half-written output
//! * a failing file never aborts the batch

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::compiler::Job;
use crate::probe::ffmpeg_bin;

#[derive(Debug, Clone)]
pub enum RunEvent {
    Started {
        id: usize,
    },
    Progress {
        id: usize,
        pct: f32,
        out_time: f64,
        speed: String,
    },
    Finished {
        id: usize,
        ok: bool,
        cancelled: bool,
        error: Option<String>,
        secs: f64,
        output: PathBuf,
    },
    BatchDone {
        ok: usize,
        failed: usize,
        cancelled: usize,
    },
}

/// Accumulates `-progress` key/value lines.
#[derive(Debug, Default)]
struct ProgressParser {
    out_time: f64,
    speed: String,
}

impl ProgressParser {
    /// Feed one stdout line; returns a progress update when a block completes.
    fn feed(&mut self, line: &str) -> Option<(f32, f64, String)> {
        let (key, value) = line.split_once('=')?;
        match key.trim() {
            "out_time_us" | "out_time_ms" => {
                // Both keys carry microseconds in practice.
                if let Ok(us) = value.trim().parse::<i64>() {
                    self.out_time = us as f64 / 1_000_000.0;
                }
                None
            }
            "out_time" => {
                if let Some(secs) = parse_timestamp(value.trim()) {
                    self.out_time = secs;
                }
                None
            }
            "speed" => {
                self.speed = value.trim().to_string();
                None
            }
            "progress" => {
                let pct = if value.trim() == "end" { 100.0 } else { f32::NAN };
                Some((pct, self.out_time, self.speed.clone()))
            }
            _ => None,
        }
    }
}

fn parse_timestamp(s: &str) -> Option<f64> {
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

pub struct Runner {
    rt: tokio::runtime::Runtime,
    tx: mpsc::UnboundedSender<RunEvent>,
    rx: mpsc::UnboundedReceiver<RunEvent>,
    cancel: CancellationToken,
    running: bool,
}

impl Runner {
    pub fn new() -> anyhow::Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let (tx, rx) = mpsc::unbounded_channel();
        Ok(Self {
            rt,
            tx,
            rx,
            cancel: CancellationToken::new(),
            running: false,
        })
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Kick off a batch. Returns immediately; poll with [`Runner::poll`].
    pub fn start(&mut self, jobs: Vec<Job>, concurrency: usize) {
        if self.running {
            return;
        }
        self.cancel = CancellationToken::new();
        self.running = true;
        let tx = self.tx.clone();
        let cancel = self.cancel.clone();
        self.rt.spawn(async move {
            run_batch(jobs, concurrency.max(1), tx, cancel).await;
        });
    }

    pub fn cancel_all(&self) {
        self.cancel.cancel();
    }

    /// Drain all events produced since the last call (never blocks).
    pub fn poll(&mut self) -> Vec<RunEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            if matches!(ev, RunEvent::BatchDone { .. }) {
                self.running = false;
            }
            out.push(ev);
        }
        out
    }

    /// Probe a file on the background runtime; the result is delivered through
    /// `tx` so the UI thread never blocks on a subprocess.
    pub fn spawn_probe(
        &self,
        path: std::path::PathBuf,
        tx: mpsc::UnboundedSender<crate::probe::ProbeResult>,
    ) {
        self.rt.spawn(async move {
            let result = crate::probe::probe(&path).await;
            let _ = tx.send(crate::probe::ProbeResult { path, result });
        });
    }
}

async fn run_batch(
    jobs: Vec<Job>,
    concurrency: usize,
    tx: mpsc::UnboundedSender<RunEvent>,
    cancel: CancellationToken,
) {
    let sem = Arc::new(Semaphore::new(concurrency));
    let ok = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for job in jobs {
        let sem = sem.clone();
        let tx = tx.clone();
        let cancel = cancel.clone();
        let ok = ok.clone();
        let failed = failed.clone();
        let cancelled = cancelled.clone();

        handles.push(tokio::spawn(async move {
            let permit = tokio::select! {
                biased;
                _ = cancel.cancelled() => None,
                p = sem.acquire_owned() => p.ok(),
            };
            let Some(_permit) = permit else {
                cancelled.fetch_add(1, Ordering::Relaxed);
                let _ = tx.send(RunEvent::Finished {
                    id: job.id,
                    ok: false,
                    cancelled: true,
                    error: None,
                    secs: 0.0,
                    output: job.output.clone(),
                });
                return;
            };

            let res = run_one(&job, &tx, &cancel).await;
            match res {
                JobOutcome::Success => {
                    ok.fetch_add(1, Ordering::Relaxed);
                }
                JobOutcome::Failed => {
                    failed.fetch_add(1, Ordering::Relaxed);
                }
                JobOutcome::Cancelled => {
                    cancelled.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let _ = tx.send(RunEvent::BatchDone {
        ok: ok.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        cancelled: cancelled.load(Ordering::Relaxed),
    });
}

enum JobOutcome {
    Success,
    Failed,
    Cancelled,
}

async fn run_one(job: &Job, tx: &mpsc::UnboundedSender<RunEvent>, cancel: &CancellationToken) -> JobOutcome {
    let started = Instant::now();
    let _ = tx.send(RunEvent::Started { id: job.id });

    let n = job.stages.len().max(1);
    let span = 100.0 / n as f32;

    for (i, stage) in job.stages.iter().enumerate() {
        match run_stage(
            job.id,
            stage,
            job.duration,
            tx,
            cancel,
            span * i as f32,
            span,
        )
        .await
        {
            StageOutcome::Ok => {}
            StageOutcome::Failed(error) => {
                // Remove everything this job wrote, including earlier stages.
                for s in job.stages.iter().take(i + 1) {
                    let _ = tokio::fs::remove_file(&s.output).await;
                }
                let _ = tx.send(RunEvent::Finished {
                    id: job.id,
                    ok: false,
                    cancelled: false,
                    error: Some(error),
                    secs: started.elapsed().as_secs_f64(),
                    output: job.output.clone(),
                });
                return JobOutcome::Failed;
            }
            StageOutcome::Cancelled => {
                for s in job.stages.iter().take(i + 1) {
                    let _ = tokio::fs::remove_file(&s.output).await;
                }
                let _ = tx.send(RunEvent::Finished {
                    id: job.id,
                    ok: false,
                    cancelled: true,
                    error: None,
                    secs: started.elapsed().as_secs_f64(),
                    output: job.output.clone(),
                });
                return JobOutcome::Cancelled;
            }
        }
    }

    // Intermediates are scratch files; the final stage's output stays.
    for s in job.stages.iter().take(n.saturating_sub(1)) {
        let _ = tokio::fs::remove_file(&s.output).await;
    }

    let _ = tx.send(RunEvent::Finished {
        id: job.id,
        ok: true,
        cancelled: false,
        error: None,
        secs: started.elapsed().as_secs_f64(),
        output: job.output.clone(),
    });
    JobOutcome::Success
}

enum StageOutcome {
    Ok,
    Failed(String),
    Cancelled,
}

/// Run one ffmpeg invocation and map its progress into `[offset, offset+span]`.
async fn run_stage(
    job_id: usize,
    stage: &crate::compiler::Stage,
    duration: f64,
    tx: &mpsc::UnboundedSender<RunEvent>,
    cancel: &CancellationToken,
    offset: f32,
    span: f32,
) -> StageOutcome {
    if let Some(parent) = stage.output.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let mut cmd = Command::new(ffmpeg_bin());
    cmd.args(&stage.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return StageOutcome::Failed(format!(
                "could not start {}: {}",
                ffmpeg_bin(),
                e
            ))
        }
    };

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    // Drain stderr concurrently so the pipe never blocks, keeping the tail for
    // the error summary.
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            tail.push(line);
            if tail.len() > 40 {
                tail.remove(0);
            }
        }
        tail.join("\n")
    });

    let mut reader = BufReader::new(stdout).lines();
    let mut parser = ProgressParser::default();
    let mut was_cancelled = false;

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                was_cancelled = true;
                let _ = child.kill().await;
                break;
            }
            line = reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some((pct, out_time, speed)) = parser.feed(&l) {
                            let local = if pct.is_nan() {
                                if duration > 0.0 {
                                    ((out_time / duration) as f32 * 100.0).clamp(0.0, 99.5)
                                } else {
                                    0.0
                                }
                            } else {
                                pct
                            };
                            let _ = tx.send(RunEvent::Progress {
                                id: job_id,
                                pct: offset + local * span / 100.0,
                                out_time,
                                speed,
                            });
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    let status = child.wait().await;
    let stderr_tail = stderr_task.await.unwrap_or_default();

    if was_cancelled {
        let _ = tokio::fs::remove_file(&stage.output).await;
        return StageOutcome::Cancelled;
    }

    if !matches!(&status, Ok(s) if s.success()) {
        // Never leave a half-written file behind.
        let _ = tokio::fs::remove_file(&stage.output).await;
        let mut msg = stderr_tail.trim().to_string();
        if msg.is_empty() {
            msg = match &status {
                Ok(s) => format!("ffmpeg exited with {}", s),
                Err(e) => format!("failed to wait for ffmpeg: {}", e),
            };
        }
        return StageOutcome::Failed(msg);
    }

    StageOutcome::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_block() {
        let mut p = ProgressParser::default();
        assert!(p.feed("out_time_us=2500000").is_none());
        assert!(p.feed("speed=1.5x").is_none());
        let (pct, t, speed) = p.feed("progress=continue").unwrap();
        assert!(pct.is_nan());
        assert!((t - 2.5).abs() < 1e-6);
        assert_eq!(speed, "1.5x");
        let (pct, _, _) = p.feed("progress=end").unwrap();
        assert_eq!(pct, 100.0);
    }

    #[test]
    fn parses_timestamp() {
        assert!((parse_timestamp("00:01:02.500000").unwrap() - 62.5).abs() < 1e-6);
    }
}
