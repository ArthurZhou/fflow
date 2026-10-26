//! Offscreen UI rendering.
//!
//! Every OS-level way of grabbing this window was tried and does not work in a
//! disconnected RDP session: `gdigrab` and `ddagrab` have no display output at
//! all, `Windows.Graphics.Capture` initialises fine but DWM never composes so no
//! frame ever arrives, and `PrintWindow(PW_RENDERFULLCONTENT)` returns the
//! window frame with a black client area because the content is an OpenGL swap
//! chain rather than a GDI surface.
//!
//! egui's output, however, is just a list of textured triangles, so the whole UI
//! can be rasterized in software without a GPU or a window.
//!
//! This module runs one frame of the real app, tessellates it, rasterizes the
//! triangles (bilinear texture sampling, premultiplied sRGB blending) and writes
//! a binary PPM. `ffmpeg -i shot.ppm shot.png` converts it.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::epaint::{ImageData, Primitive, TextureId};

// ---------------------------------------------------------------------------
// Texture atlas
// ---------------------------------------------------------------------------

struct Tex {
    w: usize,
    h: usize,
    /// premultiplied sRGBA
    px: Vec<[u8; 4]>,
}

impl Tex {
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if self.w == 0 || self.h == 0 {
            return [1.0; 4];
        }
        // egui UVs are normalized; clamp instead of wrapping so the atlas
        // border never bleeds.
        let x = (u * self.w as f32 - 0.5).max(0.0);
        let y = (v * self.h as f32 - 0.5).max(0.0);
        let x0 = (x.floor() as usize).min(self.w - 1);
        let y0 = (y.floor() as usize).min(self.h - 1);
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let fx = (x - x0 as f32).clamp(0.0, 1.0);
        let fy = (y - y0 as f32).clamp(0.0, 1.0);

        let at = |xx: usize, yy: usize| self.px[yy * self.w + xx];
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        let mut out = [0.0f32; 4];
        for i in 0..4 {
            let top = a[i] as f32 * (1.0 - fx) + b[i] as f32 * fx;
            let bot = c[i] as f32 * (1.0 - fx) + d[i] as f32 * fx;
            out[i] = (top * (1.0 - fy) + bot * fy) / 255.0;
        }
        out
    }
}

#[derive(Default)]
struct Atlases(HashMap<TextureId, Tex>);

impl Atlases {
    fn apply(&mut self, delta: &egui::TexturesDelta) {
        for (id, d) in &delta.set {
            let (w, h, px) = match &d.image {
                ImageData::Color(c) => (
                    c.size[0],
                    c.size[1],
                    c.pixels.iter().map(|c| c.to_array()).collect::<Vec<_>>(),
                ),
                ImageData::Font(f) => (
                    f.size[0],
                    f.size[1],
                    f.srgba_pixels(None).map(|c| c.to_array()).collect(),
                ),
            };
            match d.pos {
                None => {
                    self.0.insert(*id, Tex { w, h, px });
                }
                Some([ox, oy]) => {
                    if let Some(t) = self.0.get_mut(id) {
                        for row in 0..h {
                            for col in 0..w {
                                let (dx, dy) = (ox + col, oy + row);
                                if dx < t.w && dy < t.h {
                                    t.px[dy * t.w + dx] = px[row * w + col];
                                }
                            }
                        }
                    }
                }
            }
        }
        for id in &delta.free {
            self.0.remove(id);
        }
    }
}

// ---------------------------------------------------------------------------
// Framebuffer
// ---------------------------------------------------------------------------

pub struct Shot {
    pub width: usize,
    pub height: usize,
    /// premultiplied sRGB in 0..=1
    px: Vec<[f32; 4]>,
}

impl Shot {
    fn new(width: usize, height: usize, bg: egui::Color32) -> Self {
        let c = bg.to_array();
        let bg = [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            1.0,
        ];
        Self {
            width,
            height,
            px: vec![bg; width * height],
        }
    }

    /// `src` over `dst`, both premultiplied.
    #[inline]
    fn blend(&mut self, x: usize, y: usize, src: [f32; 4]) {
        if src[3] <= 0.001 {
            return;
        }
        let d = &mut self.px[y * self.width + x];
        let inv = 1.0 - src[3];
        d[0] = (src[0] + d[0] * inv).min(1.0);
        d[1] = (src[1] + d[1] * inv).min(1.0);
        d[2] = (src[2] + d[2] * inv).min(1.0);
        d[3] = (src[3] + d[3] * inv).min(1.0);
    }

    fn draw_mesh(&mut self, mesh: &egui::Mesh, tex: &Tex, clip: egui::Rect, ppp: f32) {
        let x0 = (clip.min.x * ppp).floor().max(0.0) as usize;
        let y0 = (clip.min.y * ppp).floor().max(0.0) as usize;
        let x1 = (clip.max.x * ppp).ceil().min(self.width as f32) as usize;
        let y1 = (clip.max.y * ppp).ceil().min(self.height as f32) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        for tri in mesh.indices.chunks_exact(3) {
            let v = [
                &mesh.vertices[tri[0] as usize],
                &mesh.vertices[tri[1] as usize],
                &mesh.vertices[tri[2] as usize],
            ];
            let p: [egui::Pos2; 3] = [
                egui::pos2(v[0].pos.x * ppp, v[0].pos.y * ppp),
                egui::pos2(v[1].pos.x * ppp, v[1].pos.y * ppp),
                egui::pos2(v[2].pos.x * ppp, v[2].pos.y * ppp),
            ];
            let edge = |a: egui::Pos2, b: egui::Pos2, c: egui::Pos2| {
                (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
            };
            let mut area = edge(p[0], p[1], p[2]);
            if area == 0.0 {
                continue;
            }
            let flip = area < 0.0;
            if flip {
                area = -area;
            }

            let min_x = p[0].x.min(p[1].x).min(p[2].x).floor().max(x0 as f32) as usize;
            let max_x = (p[0].x.max(p[1].x).max(p[2].x).ceil() as usize).min(x1.saturating_sub(1));
            let min_y = p[0].y.min(p[1].y).min(p[2].y).floor().max(y0 as f32) as usize;
            let max_y = (p[0].y.max(p[1].y).max(p[2].y).ceil() as usize).min(y1.saturating_sub(1));

            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let c = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                    let mut w0 = edge(p[1], p[2], c);
                    let mut w1 = edge(p[2], p[0], c);
                    let mut w2 = edge(p[0], p[1], c);
                    if flip {
                        w0 = -w0;
                        w1 = -w1;
                        w2 = -w2;
                    }
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let (l0, l1, l2) = (w0 / area, w1 / area, w2 / area);

                    let u = v[0].uv.x * l0 + v[1].uv.x * l1 + v[2].uv.x * l2;
                    let vv = v[0].uv.y * l0 + v[1].uv.y * l1 + v[2].uv.y * l2;
                    let t = tex.sample(u, vv);

                    let mut col = [0.0f32; 4];
                    for i in 0..4 {
                        let cc = (v[0].color.to_array()[i] as f32) * l0
                            + (v[1].color.to_array()[i] as f32) * l1
                            + (v[2].color.to_array()[i] as f32) * l2;
                        col[i] = cc / 255.0 * t[i];
                    }
                    self.blend(x, y, col);
                }
            }
        }
    }

    /// Binary PPM (P6). `ffmpeg` turns it into a PNG.
    pub fn write_ppm(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = Vec::with_capacity(self.width * self.height * 3 + 32);
        out.extend_from_slice(format!("P6\n{} {}\n255\n", self.width, self.height).as_bytes());
        for p in &self.px {
            // un-premultiply then encode: the alpha channel is already folded
            // into the colour by `blend`, so this is a straight sRGB copy.
            for i in 0..3 {
                out.push((p[i].clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        let mut f = std::fs::File::create(path)?;
        f.write_all(&out)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub struct Options {
    pub out: std::path::PathBuf,
    pub width: usize,
    pub height: usize,
    /// Select the n-th node so its inspector is visible.
    pub select: Option<usize>,
    /// Point the Source node at this folder.
    pub dir: Option<std::path::PathBuf>,
    /// Pre-load the probe of this file (drives the Demux track list).
    pub probe: Option<std::path::PathBuf>,
    /// Run encoder detection first, so hardware info is real.
    pub detect: bool,
    /// Populate the task list with sample jobs (one per status) so the task
    /// panel can be inspected without running a real batch.
    pub demo_tasks: bool,
    /// Open the help window before the capture (used to check its layout).
    pub show_help: bool,
    /// Force the light palette, to check that both themes lay out correctly.
    pub light: bool,
    /// Load this workflow file before rendering.
    pub workflow: Option<std::path::PathBuf>,
    /// Canvas zoom to render at (1.0 = default). Used to check that cards keep
    /// their proportions when the canvas is scaled.
    pub zoom: Option<f32>,
    /// Turn on the flow preview for the n-th file, as if it had been clicked.
    pub flow: Option<usize>,
}

/// Run one UI frame and tessellate it.
fn run_frame(
    ctx: &egui::Context,
    app: &mut crate::app::FflowApp,
    input: &egui::RawInput,
    atlases: &mut Atlases,
    primitives: &mut Vec<egui::ClippedPrimitive>,
    ppp: &mut f32,
) {
    let out = ctx.run(input.clone(), |c| app.draw_all(c));
    atlases.apply(&out.textures_delta);
    *ppp = out.pixels_per_point;
    *primitives = ctx.tessellate(out.shapes, *ppp);
}

pub fn render(opts: &Options) -> anyhow::Result<()> {
    if opts.detect {
        crate::encoders::detect_now();
    }

    let ctx = egui::Context::default();
    crate::app::install_fonts(&ctx);
    ctx.set_pixels_per_point(1.0);

    let mut app = crate::app::FflowApp::build(&ctx, false);
    if opts.light {
        crate::theme::apply(&ctx, false);
    }
    if let Some(wf) = &opts.workflow {
        app.demo_load_workflow(wf)?;
    }
    app.demo_setup(opts.dir.as_deref(), opts.probe.as_deref())?;
    if let Some(i) = opts.flow {
        app.demo_flow(i);
    }
    if opts.demo_tasks {
        app.demo_tasks();
    }
    if opts.show_help {
        app.demo_help();
    }
    if let Some(i) = opts.select {
        anyhow::ensure!(app.select_node(i), "no node #{i} in the graph");
    }

    let mut input = egui::RawInput::default();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(opts.width as f32, opts.height as f32),
    ));

    let mut atlases = Atlases::default();
    let mut primitives: Vec<egui::ClippedPrimitive> = Vec::new();
    let mut ppp = 1.0f32;
    // A few frames: egui needs one to settle the layout (panels report their
    // sizes on the first pass) before the capture is representative.
    for _ in 0..4 {
        run_frame(&ctx, &mut app, &input, &mut atlases, &mut primitives, &mut ppp);
    }
    // Zoom only after the first frames: it rescales node positions around the
    // canvas centre, and the canvas has no size before it has been laid out.
    if let Some(z) = opts.zoom {
        app.demo_set_zoom(&ctx, z);
        for _ in 0..4 {
            run_frame(&ctx, &mut app, &input, &mut atlases, &mut primitives, &mut ppp);
        }
    }
    // Probing and encoder detection are real background work; without waiting
    // for them the capture would show "probe pending" and an empty command
    // preview, which is not what the app actually looks like in use.
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut waited = 0u32;
    while app.is_busy() && Instant::now() < deadline {
        run_frame(&ctx, &mut app, &input, &mut atlases, &mut primitives, &mut ppp);
        waited += 1;
        std::thread::sleep(Duration::from_millis(20));
    }
    if waited > 0 {
        eprintln!("[shot] waited {waited} frame(s) for background work");
    }
    run_frame(&ctx, &mut app, &input, &mut atlases, &mut primitives, &mut ppp);

    let bg = ctx.style().visuals.panel_fill;
    let mut shot = Shot::new(opts.width, opts.height, bg);
    let mut meshes = 0usize;
    let mut tris = 0usize;
    for prim in &primitives {
        if let Primitive::Mesh(mesh) = &prim.primitive {
            let Some(tex) = atlases.0.get(&mesh.texture_id) else {
                continue;
            };
            shot.draw_mesh(mesh, tex, prim.clip_rect, ppp);
            meshes += 1;
            tris += mesh.indices.len() / 3;
        }
    }

    shot.write_ppm(&opts.out)?;
    println!(
        "wrote {} ({}x{}, {} meshes, {} triangles)",
        opts.out.display(),
        opts.width,
        opts.height,
        meshes,
        tris
    );
    Ok(())
}
