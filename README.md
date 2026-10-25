# fflow

A Scratch-like visual front-end for **batch ffmpeg** processing. Pick a root folder,
wire nodes together on a canvas, and every media file under that folder is pushed
through the same pipeline — with a live command preview and per-file progress.

Pure Rust desktop app: **eframe/egui**, no web front-end, no Tauri, no libav
bindings. All media work happens in `ffmpeg` / `ffprobe` child processes.

```
┌─ toolbar ─────────────────────────────────────────────────────────────────────┐
│ fflow · name │ 📁 Root folder │ ▶ Run  ■ Cancel │ Save  Save as…  Load  New │ … │
├──────────────┬────────────────────────────────────────────┬───────────────────┤
│ NODE LIBRARY │                                            │ INSPECTOR         │
│ INPUT    2   │        node graph canvas (dot grid)        │ ▎ Video Encoder   │
│ ▎📁 Source   │   Source → Demux → filters → encoders      │  Codec  [libx264] │
│ ▎⑂ Demux    │                    → Package               │  CRF    [23]      │
│ FILTERS 17   │                                            │  ⚡ runs as nvenc  │
│ ▎🖼 Scale    │                          ● File ● Video …  │                   │
│ ─────────────│                                            │                   │
│ FILES 3      │                          100% · 5 nodes    │ COMPILED OUTPUT   │
│ clip1.mp4    │                                            │                   │
├──────────────┴────────────────────────────────────────────┴───────────────────┤
│ COMMAND PREVIEW  ◀ clip1.mp4 ▶   1/3                              Copy        │
│ ffmpeg -hide_banner … -filter_complex … -c:v h264_nvenc …                    │
├───────────────────────────────────────────────────────────────────────────────┤
│ TASKS  1 running  1 done  1 failed                    ▓▓▓▓▓░░░ 81% overall    │
│    FILE            PROGRESS        STATUS   DETAIL                            │
│ ●  clip1.mp4       ▓▓▓▓▓▓ 100%     done     1.2s                    open      │
│ ●  clip2.mp4       ▓▓░ 42%         running  1.9x · 1.3s / 3.0s                │
└───────────────────────────────────────────────────────────────────────────────┘
```

## UI

The layout is three columns over two stacked bottom panels:

* **Toolbar** — grouped by dividers: brand + workflow name · input (root
  folder) · run/cancel · workflow files · status cluster (parallelism, file
  count, hardware encoders, palette, help). `Run` is the single filled button;
  `Cancel` only lights up while a batch is running. The status cluster is
  right-aligned when there is room and drops to a second toolbar row on a
  narrow window instead of overlapping the buttons on the left.
* **Node library** (left, top) — one card per node kind, grouped into foldable
  categories with a count each (`Input 2`, `Filters 17`, …). The search box
  matches titles, descriptions and a keyword list, so `resize`, `loudness` or
  `watermark` find the right card even though those words are not on it; a
  search temporarily expands every group it matched. The colour bar on the left
  identifies the node's family, so the video branch (orange) and the audio
  branch (green) are recognisable before you build anything. Click to add at
  the canvas centre, or drag onto the canvas.
* **Files** (left, bottom) — the files the Source node resolves to, with the
  probed stream summary. Click a row to preview that file's command. This is
  the list that actually gets processed; without it the only feedback was a
  count.
* **Canvas** — dot grid that pans and zooms with the graph, node title bars
  coloured by media type, a hairline border on every card, and a status overlay
  in the corner (port legend, zoom, node count). On the first frame the graph is
  zoomed to fit the canvas and centred, so a fresh window shows the whole
  pipeline instead of cards disappearing under the side panels; after that the
  view is yours. Drag empty canvas to box-select, middle-drag (or cmd-drag) to
  pan, wheel to zoom, `Del`/`Backspace` to delete the selection, `Esc` to
  deselect.
* **Inspector** (right, 360 px by default) — node header (icon, title, category,
  `Delete`), then the property form in a two-column grid with a fixed label
  gutter (segmented controls put their caption above the buttons instead, so
  four options fit without wrapping), then what this node compiles to right now.
* **Command preview** — the real argv, tokenised and coloured (options, codec
  values, filter graph, paths, output), with copy and per-file navigation.
* **Tasks** — aligned table with a per-status pill summary, an overall progress
  bar, per-row progress, speed / elapsed, and an expandable error block.

Everything is built from a small design system in `theme.rs` (palette, type
scale, spacing, and widgets like `section`, `field`, `pill`, `progress`,
`card`), so panels do not invent their own colours or spacing.

The look is **shadcn/ui**: the zinc palette (near-black `#09090B` canvas, 4-step
surface ladder, `#FAFAFA` text), 6 px radii, one accent colour per meaning, and
borders used sparingly — only inputs, combos and the selected node get a
hairline; panels, cards and buttons are separated by a one-shade surface step
instead of an outline. Media colours are kept on purpose (video orange, audio
green, file blue, logic amber) because they carry information. Dark and light
palettes both ship; the `Dark`/`Light` button in the toolbar switches between
them and remembers the choice, and `--screenshot --light` renders the light one.

All five dividers (library, files, inspector, preview, tasks) are draggable and
their sizes persist in the settings file. Every panel also has a sensible
minimum: the canvas never drops below 320 px, and the bottom panels give up
height first when the window shrinks.

## Build & run

Requires a Rust toolchain and `ffmpeg`/`ffprobe` on `PATH`
(override with `FFLOW_FFMPEG` / `FFLOW_FFPROBE`).

```bash
cd fflow
cargo run --release          # launch the GUI
cargo test                   # 63 unit + headless-UI tests
cargo run -- --selftest testdata      # headless end-to-end check on a folder
cargo run -- --screenshot ui.ppm --size 1600x980   # render the UI without a display
```

`--selftest <dir>` builds the default pipeline, probes every file, compiles the
commands, runs the batch, verifies the outputs, then runs a cancellation test and
a multi-track test. It is the fastest way to confirm a machine is set up
correctly.

`--screenshot <out.ppm>` runs the real `FflowApp` against an offscreen egui
context, rasterises the resulting meshes in software and writes a PPM. It needs
no window, no GPU and no display server, which makes it usable over a headless /
disconnected RDP session. Options: `--select N` clicks a node first (so an
inspector form is visible), `--dir` / `--probe` seed the file list and the
command preview, `--workflow <file>` renders a specific saved graph (that is how
the Condition-branch screenshots are made), `--demo-tasks` fills the task table
with one job per status, `--light` renders the light palette, and `--zoom 0.6`
renders the canvas at that zoom (through the same path the wheel uses, so node
positions are rescaled like they are in the app). `ffmpeg -i shot.ppm shot.png`
converts.

## Usage

1. **📁 Root folder** — pick a directory (or drop files/folders onto the canvas).
   The Source node resolves it with `walkdir` (recursive toggle + extension filter).
2. Build a pipeline: `Source → Demux → (Scale → Video Encoder) / (Audio Encoder) → Package`.
   The default workflow already contains exactly that; edit it instead of starting from scratch.
3. Click a node to edit it in the inspector. Every change re-compiles the preview
   at the bottom immediately — that panel always shows the *exact* command line.
4. **▶ Run** — one ffmpeg process per file, at most `Parallel` at a time
   (default 2). Progress bars are driven by ffmpeg's `-progress pipe:1` output.
5. **■ Cancel all** — kills every child process and deletes half-written outputs.
6. **💾 Save / 📂 Load** — the workflow is plain JSON (see below). The last session
   is auto-restored on the next launch.

## Node reference

Every card does **one** thing. A step that used to be a checkbox inside a big
"Video Filter" card is now its own node, so the canvas reads as the filtergraph
it produces and each inspector only shows the handful of controls that belong
to that step. The colour family is unchanged: file = blue, video = orange,
audio = green, logic = amber, output = violet.

| Node | In → Out | Config |
|---|---|---|
| **Input** | | |
| Source | – → `file` (File) | paths (folders/files), recursive, extension filter, skip own outputs |
| Demux | `in` (File) → `video`, `audio` | per-type track mode: first / all / select / off (+ subtitle mapping) |
| **Filters · video** | | |
| Scale | `in` → `out` (Video) | width/height + exact / fit / fill, presets |
| Pad | `in` → `out` (Video) | canvas size, x/y, colour (accepts `(ow-iw)/2`) |
| Crop | `in` → `out` (Video) | width, height, x, y (accepts `iw`/`ih`) |
| Rotate / Flip | `in` → `out` (Video) | 90° / 180° / 270° + mirror |
| Frame Rate | `in` → `out` (Video) | target fps |
| Colour | `in` → `out` (Video) | brightness, contrast, saturation, gamma (neutral = passthrough) |
| 3D LUT | `in` → `out` (Video) | `.cube`/`.3dl` path + interpolation |
| Sharpen | `in` → `out` (Video) | amount |
| Denoise | `in` → `out` (Video) | strength |
| Overlay | `in` → `out` (Video) | image path, x, y, pre-scale (adds a second `-i`) |
| **Filters · audio** | | |
| Volume | `in` → `out` (Audio) | gain in dB (0 dB = passthrough) |
| Loudness | `in` → `out` (Audio) | integrated LUFS target + true-peak ceiling |
| Band | `in` → `out` (Audio) | high-pass / low-pass in Hz (0 = off) |
| Tempo | `in` → `out` (Audio) | playback factor (0.5 … 2) |
| Channels | `in` → `out` (Audio) | mono / stereo / 5.1 |
| Resample | `in` → `out` (Audio) | sample rate |
| Raw | `in` → `out` (Video **or** Audio) | one verbatim filtergraph fragment |
| **Logic** | | |
| Condition | `in` (Any) → `pass`, `fail` (Any) | rule list (field / operator / value), all-or-any, per-file evaluation |
| **Encoders** | | |
| Video Encoder | `in` (Video) → `out` | codec, crf **or** bitrate, preset, pixel format, prefer hardware |
| Audio Encoder | `in` (Audio) → `out` | codec, bitrate, channels, sample rate |
| **Output** | | |
| Package | `video`, `audio` (fan-in) → `file` | container, `-movflags +faststart`, `-shortest`, metadata, name template, output dir, extra args |

Port types are enforced: **File → Video** (or any other mismatch) is refused the
moment you drop the connection, and a connection that would close a cycle is
refused too. `Condition` and `Raw` are the exceptions — their ports are typed
`Any`, which is compatible with every other type, so the same node can sit in a
video or an audio chain. Package needs at least one connected input (it says so
on the node and the compiler reports it).

**Port labels follow the signal.** Inputs are labelled on the left edge, outputs
on the right, so a card reads left-to-right like the data flows through it.

**Merge points.** The encoder and Package inputs accept several connections
(`fan-in`): that is how the two sides of a Condition re-join, and how a Package
can collect a video and an audio branch without an extra node. At compile time
exactly one live branch per file may reach such an input — two unrelated live
branches is reported as an ambiguity instead of being silently guessed.

Name template tokens: `{dir} {stem} {name} {ext} {index} {rel_dir}`
(default `{dir}/{stem}_fflow.{ext}`). The compiler refuses to write over the
source file.

## Filter cards

Each filter card emits one filter (or one tightly-coupled pair, like
`transpose` + `hflip`) and the **wiring decides the order** — there is no
hidden canonical sequence any more. Chain them left to right:

```
Demux ─video─▶ Crop ─▶ Scale ─▶ Colour ─▶ Video Encoder ─▶ Package
```

| Card | Filter |
|---|---|
| Scale · exact | `scale=w:h` (`-2` keeps the ratio) |
| Scale · fit | `scale=w:h:force_original_aspect_ratio=decrease` |
| Scale · fill | `…=increase` then `crop=w:h` |
| Pad | `pad=w:h:x:y:color=…` |
| Crop | `crop=w:h:x:y` (accepts `iw`/`ih` expressions) |
| Rotate / Flip | `transpose=1` / `transpose=2` / `hflip` / `vflip` |
| Frame Rate | `fps=N` |
| Colour | `eq=brightness:contrast:saturation:gamma` |
| 3D LUT | `lut3d=file=…:interp=nearest\|trilinear\|tetrahedral` |
| Sharpen | `unsharp=5:5:amount:5:5:0` |
| Denoise | `hqdn3d=strength` |
| Overlay | `[1:v]overlay=x:y:shortest=1` (extra `-i`, optional pre-scale) |
| Volume | `volume=±NdB` |
| Loudness | `loudnorm=I=…:TP=…:LRA=11` |
| Band | `highpass=f=…` / `lowpass=f=…` |
| Tempo | `atempo=factor` |
| Channels | `aformat=channel_layouts=…` |
| Resample | `aresample=rate` |
| Raw | verbatim fragment, rejected if it contains `;` |

* A card with neutral settings is a **passthrough**: it contributes no filter at
  all and the branch is mapped straight from the source, so dropping an empty
  card into the graph never adds a needless re-encode.
* Paths inside a filtergraph are escaped (`C:/luts/look.cube` →
  `C\\:/luts/look.cube`). A filter option value is parsed **twice** — once by
  the filtergraph parser, once by the filter's own option parser — and both
  treat `:` as a separator, so the drive colon needs a double escape. Single
  escaping compiles fine and then fails at runtime with
  `No option name near '/luts/look.cube'`; the test asserts the doubled form.
* Invalid combinations are caught before ffmpeg runs: `fill` needs both a width
  and a height, an enabled LUT needs a file, and each numeric control has a
  range that `problems()` checks.

## Conditional branches

`Condition` takes the file **into** `in` and lets it out of exactly one of two
ports — `pass` when the rules match, `fail` when they do not:

```
                       ┌── pass ──▶ Scale 1920 ──┐
 Demux ─video─▶ in  Condition                    ├──▶ Video Encoder ─▶ Package
                       └── fail ──▶ Scale 640  ──┘
```

Both sides can re-join at the encoder or at the Package node, because those
inputs accept several connections. The decision is made **per file at compile
time**, so the preview always shows the command that file will actually run —
there is no ffmpeg-side switch.

* Rules are editable rows: a **field** (file name, extension, full path, size,
  duration, width, height, fps, video/audio codec, bitrate, has video/audio, or
  any container metadata tag), an **operator** (`contains`, `does not contain`,
  `starts with`, `ends with`, `is`, `matches` (glob), `>`, `<`) and a **value**.
  Numeric fields only offer numeric operators; `metadata` adds a key field.
* `all` = AND, `any` = OR. No rules at all means "always pass".
* Matching is case-insensitive; `matches` is a dependency-free `*` / `?` glob.
* A file that lands on an **unconnected** branch is **skipped**, not failed: the
  batch counts it separately (`N file(s) skipped by a condition`) and the
  preview says `skipped — …`. If the other branch (say audio) is still wired up,
  that part is still produced and a note explains the dropped half.

## Multi-stage workflows

A Package node has a `file` output. Wire it into another Demux and the workflow
keeps processing the file it just wrote:

```
Source ─▶ Demux ─▶ … ─▶ Package ──file──▶ Demux ─▶ … ─▶ Package
         (stage 1, intermediate)                  (stage 2, final output)
```

The compiler turns each hop into its own ffmpeg invocation and the runner runs
them in order, mapping progress across the whole job (stage 1 fills the first
half of the bar, stage 2 the rest). Intermediates are written to
`%TEMP%/fflow/<stem>-<index>-s<N>.<ext>` and deleted when the job finishes;
cancelling or failing removes them as well as the partial output. The preview
shows every stage as `[1] ffmpeg …` / `[2] ffmpeg …`. A workflow needs exactly
one Package whose `file` output is unconnected — that is the final output.

## Generated commands

```
ffmpeg -hide_banner -nostats -loglevel error -progress pipe:1 -y
       -i <input> [-i overlay.png …]
       -filter_complex "[0:v]scale=1280:-2[v];[0:a]volume=1.5[a]"
       -map "[v]" -map "[a]"
       -c:v libx264 -crf 23 -preset medium -pix_fmt yuv420p
       -c:a aac -b:a 192k -ac 2 -ar 48000
       -f mp4 -movflags +faststart <output>
```

* One `-filter_complex` holds every branch, chained with labels
  (`[0:v]…[t0];[t0][1:v]overlay=…[v]` when an overlay pulls in an extra input).
* A branch without filters is mapped directly (`-map 0:a:0`), so no needless
  filter pass is inserted.
* A branch with filters but no encoder is a compile error (ffmpeg would have to
  re-encode anyway). A branch with neither is a stream copy.
* Files that turn out to have no audio/video are handled per file: the missing
  branch is skipped and noted instead of failing the job.

### Multi-track mapping

`Demux` decides which tracks reach the graph, independently per media type
(`first` / `all` / `select` / `off`). Every selected track becomes its own
branch, so a 2-audio-track file with `audio = all` compiles to:

```
-filter_complex "[0:v]scale=1280:-2[v];[0:a:0]volume=1.5[a0];[0:a:1]volume=1.5[a1]"
-map "[v]" -map "[a0]" -map "[a1]"
-c:v libx264 … -c:a aac …          # emitted once, applies to every stream of that type
```

* Source pads use ffmpeg ordinals (`[0:v]`, `[0:a:1]`), output pads are `[v]` /
  `[a]` for a single track and `[v1] [v2] …` / `[a1] [a2] …` once there are
  several, so the labels never collide.
* Ordinals are counted the way ffmpeg counts them: a cover-art stream
  (`attached_pic`) still consumes a video ordinal even though it is not mapped.
* Subtitles bypass the filter graph entirely (`-map 0:s:0` + `-c:s mov_text` for
  mp4/mov, `copy` for mkv). Audio-only containers drop them with a note on the
  node instead of producing an invalid command.

### Hardware encoders

`Video Encoder` can prefer a hardware encoder. On startup the registry is built
from `ffmpeg -encoders`, then **every** hardware candidate is verified by
actually encoding one frame to `-f null -` (4 in parallel, 10 s timeout), because
a listed encoder is not necessarily a working one. Results are cached in
`%APPDATA%/fflow/encoders.json`; the toolbar shows what was found
(`⚡ h264_nvenc, hevc_nvenc`) and the inspector has a re-detect button.

`libx264` is substituted with a verified hardware encoder for the same codec
(`h264_nvenc`, `h264_qsv`, `h264_amf`, …), and the quality flag is rewritten for
that family — NVENC uses `-rc vbr -cq N -b:v 0`, QSV `-global_quality`, AMF
`-rc cqp -qp_i/-qp_p`, VideoToolbox/MediaFoundation `-q:v`, software `-crf`.
Presets are mapped per family too (`-preset p5` vs `-quality balanced`).
VAAPI is deliberately excluded from auto-substitution: it needs an explicit
`-vaapi_device` upload chain, so a blind swap would produce a broken command.

## Workflow JSON

```json
{
  "version": "1.0",
  "name": "workflow",
  "nodes": [
    { "id": 0, "kind": "source", "config": { "paths": ["D:/in"], "recursive": true,
      "extensions": "mp4,mkv", "skip_own_outputs": true }, "pos": [40.0, 60.0] },
    { "id": 1, "kind": "demux", "config": {}, "pos": [330.0, 60.0] }
  ],
  "edges": [
    { "from": [0, "file"], "to": [1, "in"] }
  ]
}
```

`config` is opaque to the format (one struct per node kind); `pos` is optional and
only affects layout. Old files without `pos`/new fields still load.

## Source layout

```
src/
├── main.rs        eframe bootstrap, --selftest / --screenshot harnesses
├── app.rs         FflowApp state, three-pane layout, event pump, drop handling
├── theme.rs       palette, type scale, spacing, shared widgets
├── graph/
│   ├── mod.rs     egui_node_graph2 trait impls, workflow <-> editor conversion
│   ├── model.rs   Workflow / WfNode / Edge / Settings (serde)
│   ├── legacy.rs  migration of pre-split workflows (video_filter → cards)
│   ├── nodes.rs   node kinds (one KindSpec table), encoders, condition, package
│   └── nodes/
│       └── filters.rs  one config + filter + form per filter card
├── source.rs      rfd dialogs, walkdir recursion, extension + own-output filter
├── probe.rs       ffprobe JSON → ProbeInfo (per-type stream lists, ordinals)
├── encoders.rs    ffmpeg -encoders parsing, hardware probing + cache
├── compiler.rs    graph → ffmpeg argv (+ filter_complex, output template)
├── runner.rs      tokio batch, Semaphore, -progress parsing, cancellation
└── shot.rs        offscreen egui render → software rasteriser → PPM

vendor/egui_node_graph2/   upstream 0.7.0, patched: zoom-scaled port padding,
                           card border + tighter body spacing, exact node rects
                           in `GraphResponse`, themed box-selection marquee
                           (each patch is marked `fflow patch` in the source)
```

Threading model: the UI thread only draws and reads channels. Probes and ffmpeg
processes run on a 2-worker tokio runtime; results arrive over unbounded channels
and are drained with `try_recv` each frame, so the UI never blocks on a
subprocess. Encoder detection runs on its own threads and is polled the same way.

## Verification

`cargo test` — 77 tests:

| test | proves |
|---|---|
| `graph::tests` (5) | default workflow is wired correctly, type mismatch detected, cycles detected, JSON round-trip is lossless, `Any` ports match every type while File/Video/Audio stay strict |
| `graph::draw_tests` (1) | the canvas draws a connected graph without panicking (headless egui) |
| `graph::nodes` (16) | every inspector form renders (all 23 cards), demux track modes, hardware state in the encoder form, container/subtitle support, `{}` configs still load, condition rules (name / glob / number / metadata tag / OR), the glob matcher, and every filter card's emitted expression: scale modes, pad/crop, rotate/flip, LUT path double-escaping, neutral cards being passthrough, audio cards, raw rejecting a second segment |
| `graph::legacy` (4) | an old `video_filter` / `audio_filter` workflow expands into the same cards in the same order, `pad` splits into `fit` + `Pad`, the rewired graph compiles to **byte-identical** filtergraph, and an old two-input Condition becomes one Condition per branch |
| `app` (23) | every panel + every inspector form renders a full frame; missing Package is reported; nodes drag by the titlebar, click-select, box-select, delete via close button / Del / inspector button, and survive any deletion; all five splitters resize their panel and the canvas follows, and each handle sits on the edge it resizes; the file list cannot grow over the node library; a 1080x700 window fits the whole graph into the canvas; every card stays under 170x140 px; the canvas pans with the middle button and zooms with the wheel; a click in the inspector keeps the node selected; library cards add nodes by click and by drag-and-drop; a file card moves the preview; an unreadable file is not re-probed and does not block the batch; "New" rebuilds the template and drops the stale caches; zooming keeps every card the same size in graph units, in both width and height; every UI glyph exists in the bundled fonts |
| `compiler` (16) | output template tokens, overlay graph + extra input ordering, multi-track mapping, track selection, per-family quality flags, one branch per file, skip on an unconnected branch, the other stream surviving a dropped branch, and a Package → Demux chain compiling into two ordered stages with a temp intermediate |
| `encoders` (5) | `-encoders` parsing, hardware codec resolution, per-family args |
| `probe` (5) | stream detection incl. audio-only files, per-type ordinals, cover-art ordinals, track-mode resolution |
| `runner` (2) | `-progress` key/value parsing, timestamp parsing |

`--selftest` — 7 steps on a real folder:

```
[1/7] workflow JSON round-trip ok (5 nodes, 5 edges)
[2/7] encoders: 138 video / 90 audio · hardware usable: h264_nvenc, hevc_nvenc
[3/7] clip1.mp4 — v:h264 + a:aac · 320x240 @ 25.00fps · 2.00s
[4/7] ffmpeg -hide_banner … -map 0:v:0 -map 0:a:0 -c:v h264_nvenc … clip1_fflow.mp4
[5/7] batch: 3 ok, 0 failed, 0 cancelled
[6/7] cancel ok (child killed, no partial file)
[7/7] multi-track ok (1 video + 2 audio: aac, aac)
```

`--screenshot out.ppm` renders the whole UI offscreen (no window, no GPU) and is
how the dark/light shots in `shots/` are produced.

## Design decisions & deviations

* **egui, not Tauri.** The detailed spec forbids web tech; egui also keeps the
  binary self-contained. `egui_node_graph2 0.7` pins `egui 0.29`, so `eframe` is
  pinned to 0.29 rather than the newest release.
* **`shown_inline` must be `true` for inputs.** In egui_node_graph2 0.7 the
  input-port height pass is gated on `shown_inline`; with `false` no port hook is
  painted and drawing any existing connection panics with
  `no entry found for key`. Ports therefore use `shown_inline: true` and render
  their label through `WidgetValueTrait`.
* **`pos` added to the node document** (the spec's `Workflow` has no positions) so
  a reopened file looks exactly like it did.
* **Previous outputs are skipped** by default (`skip_own_outputs`), otherwise a
  second run would happily re-encode `*_fflow.mp4` into `*_fflow_fflow.mp4`.
* **One Package node** per workflow (the spec describes a single terminal). More
  than one is reported as a compile error instead of silently picking one.
* **Demux port lighting** uses the probe of the file shown in the command preview;
  per-file probing at draw time would spawn a process per file per frame. Batch
  compilation still checks every file individually.
* **`-y` and `-progress pipe:1` are emitted before the output path** (the spec
  says "append"); they are global options, so behaviour is identical and this
  ordering is unambiguous for ffmpeg.
* **A design system instead of per-panel styling.** `theme.rs` owns the palette,
  the type scale and the spacing; panels use `section` / `field` / `pill` /
  `card` / `progress` rather than raw `ui.label` + fixed-width widget pairs.
  That is what makes the inspector forms line up and the toolbar group cleanly.
  Three egui behaviours are worth remembering before touching it:
  - `ProgressBar` aligns its label with the *current* layout, which pins the
    text to the left edge inside a `horizontal` row; `theme::progress` draws the
    label centred instead (and switches its colour at 50% so it stays readable
    over both the fill and the empty track).
  - `allocate_ui_with_layout` shrinks to the size the content actually used, so
    table cells need an explicit `ui.set_min_width(w)` to reserve their column.
  - Inside a `right_to_left` layout, widgets added *later* end up further left;
    a nested `ui.horizontal` inherits that direction, so the "Parallel [2]"
    group adds the drag value first and the label second.
* **`StreamType::Any` is what makes the Condition node possible.** The graph
  editor validates a drop with `port_type == other_type`, so a `File`-typed
  node can never sit in a video chain and a `Video`-typed one can never sit in
  an audio chain. `Any` compares equal to every other variant (a hand-written
  `PartialEq`; the derive is gone), which keeps the editor's type rejection for
  everything else while letting one branch node serve both media types.
* **One card, one filter.** The alternative — a "Video Filter" card with a
  toggle per step — keeps the library short but makes the inspector a wall of
  checkboxes whose order you cannot see or change. Splitting it into 17 cards
  costs a taller library, which is what the foldable groups and the keyword
  search are for; in exchange the canvas *is* the filtergraph, the order is the
  wiring, and every inspector fits without scrolling. The colour families are
  unchanged (file / video / audio / logic / output), so nothing had to be
  re-learned.
* **Merge points use `add_wide_input_param`.** `add_input_param` forces
  `max_connections = 1`, which makes a Condition's two branches impossible to
  re-join at the encoder. The encoder and Package inputs are declared wide
  (`fan_in`) instead, and the compiler requires exactly one *live* branch per
  file at those ports. One consequence worth knowing: `Graph::connection()`
  returns `None` for any port with `max_connections != 1`, so the compiler reads
  those with `connections(input).first()`.
* **Canvas zoom scales the *style*, not a transform.** `egui_node_graph2`
  implements zoom by swapping in a `Style` scaled with its own `Scale` trait,
  and node positions are stored in screen pixels and rescaled around the canvas
  centre whenever the zoom changes. Two consequences bit us:
  - Any hand-picked size inside a card (the port-label floor, the port dot) is
    in screen pixels while everything around it is already scaled, so it must
    be multiplied by the zoom. A fixed 146 px floor made cards 247 graph px
    wide at 0.68× and 127 at 1.46× — the graph fell apart when you zoomed out.
  - The zoom is applied *inside* `draw_graph_editor`, so anything cached before
    that call is one frame stale. `right_label` therefore reads the scale back
    out of `TextStyle::Body.resolve(ui.style()).size / theme::BODY`, which is
    by definition the style the card is being laid out with.
  - The dependency has the same bug in its own layout: it compares an unscaled
    `port_height` (10 or 25 px) against a scaled row height and adds the
    difference as padding, ~25 px per inline input at any zoom. Cards grew to
    four times their natural height when zoomed out — tall narrow slabs. Fixed
    in `vendor/egui_node_graph2` (see *Source layout*), because there is no hook
    for it.
* **The ✕ is repainted, not replaced.** `egui_node_graph2` draws the close
  button as two bare strokes straight onto the node body, which disappears over
  a coloured title bar. Rather than fork the dependency, `paint_close_buttons`
  redraws a soft disc and the cross at exactly the same geometry after the graph
  is drawn; the hit area stays the library's.
* **Chain nodes are passthrough by default.** A filter node with every toggle
  off emits no filter at all and the compiler maps the source stream directly.
  The alternative — always emitting `volume=1` / `null` — would force an
  unnecessary re-encode (and an unnecessary quality loss) on anyone who dropped
  a node in to look at it.
* **The inspector form measures its width before laying the row out.** Inside a
  `horizontal` `Ui`, `available_width()` already excludes the label gutter, so
  nested wrapping layouts subtracted the gutter twice and broke `Mode exact /
  fit / fill` into two rows even in a 360 px panel. `theme::field`
  computes the control width up front and hands the child an explicit size;
  `nodes::wrap_row` then wraps against that size only when it truly must.
* **`selectable_labels` is off.** egui gives every `Label` `Sense::click_and_drag`
  by default so text can be selected. The node title is a `Label` drawn *on top
  of* the node's own drag handle, so with the default it swallowed every attempt
  to drag a node by its titlebar — the node simply would not move. Since the
  title label belongs to `egui_node_graph2` we cannot opt out per label, so the
  style disables selectable labels globally; the ffmpeg stderr in the task list
  opts back in with `Label::selectable(true)`, because that is the one piece of
  text people actually copy.
* **`Graph::remove_node` leaves dangling connection keys.** It removes the node
  and its parameters and filters the *values* of `connections`, but keeps the
  now-empty `InputId` keys in that `SecondaryMap`. The next
  `graph.get_input(that_id)` panics with `invalid SlotMap key used` — which made
  deleting any node with a connected input crash the very next frame.
  `graph::prune_dangling_connections` scrubs those keys after a deletion, and
  `invalid_connections` uses the `try_` accessors so it can never trip over a
  transiently inconsistent graph.
* **`Color32::gamma_multiply` scales alpha too.** Using it to darken a titlebar
  produced a *translucent* fill, which the near-white outline the library paints
  behind every node then washed out into a pale pastel in both themes. Titlebar
  colours now go through `theme::mix`, which always returns an opaque colour.
* **The selection ring is drawn from the editor's own rect.** The library's
  selected outline is `Color32::WHITE.lighten(0.8)` drawn behind the node —
  invisible on the light palette — and the rect we can read back
  (`Id::new((node, "window"))`) is the *hit* rect: it lags a frame and stops
  `titlebar_height / 2` short of the card's rounded bottom cap. Guessing that
  gap with a constant made the ring cut through the card on tall headers and
  float below it on short ones. `GraphResponse` now hands back the exact rect
  each card was painted into, so the ring (and box selection) use the geometry
  that is actually on screen.
* **The splitters are ours, not egui's.** egui's panel handles are registered
  *before* the `CentralPanel`, and the graph editor allocates a
  `Sense::click().union(Sense::drag())` background rect covering the whole
  canvas afterwards; hit-testing prefers the later widget, so the built-in
  handles were dead on every edge that touches the canvas. All five dividers are
  therefore custom `Area`s in `Order::Middle` created *after* the canvas, which
  both wins the pointer and lets the panels be `resizable(false)` +
  `exact_width/exact_height` from the persisted `PanelSizes`. `clamp_panels`
  keeps the canvas at its 320 px minimum by shrinking the largest panel first.
* **Splitter strips are placed from the panel rects, not from the sizes.** A
  panel's `exact_height` is applied *inside* its frame margin, so the file list
  (a panel nested in the library's padded content) starts 8 px above
  `preview_top - files`. Deriving the handle from the size put the line and the
  hit area 8 px below the divider they resize. Every panel now records the rect
  it was laid out with (`PanelRects`) and the strips are centred on those edges;
  `splitter_strips_sit_on_the_panel_edges` fails if they ever drift again.
  The same rect bounds the file list, which used to be clamped against the
  *screen* and could therefore grow straight through the node library.
* **Fit-to-view on the first frame.** The default pipeline is ~680 px wide and
  the centre panel is ~460 px on a 1080-wide window, so at 100% the last cards
  sat behind the inspector — which reads as a stacking bug, not as "the graph
  continues off-screen". The first layout zooms to fit (never below 0.45) and
  centres the bounding box; a 1440-wide window still opens at 100%.
* **Cards are compact on purpose.** 148 px wide, 87–136 px tall, 1 px
  `bg_stroke` border. The width floor in `right_label` (146 → 124) and the
  row height (22 → 16 px) were the two numbers that made every card a slab;
  `egui_node_graph2`'s 6 px item spacing is halved inside a node body, where it
  was pure padding between one-line port rows.
* **`egui_node_graph2` deselects on any click outside the editor.** Its
  `mouse.any_click() && !cursor_in_editor` branch fires for clicks on the
  inspector too — so the first click on any form control blanked the panel that
  was editing the selected node. `canvas()` snapshots the selection before
  drawing and restores it (filtered to live nodes) when the click landed
  elsewhere in the window; clicking empty canvas still deselects, because that
  path has the pointer inside the editor.
* **Only the bundled glyphs render.** egui ships Ubuntu-Light plus a monochrome
  NotoEmoji; `◐`, `→`, `●`, `✕`, `▸`, `⑂` and `🎛` are in neither and showed up
  as tofu boxes. They are now `Dark`/`Light`, `»`, `•`, `×`, `›`, `🔀`, `⚖` and
  `⚙`,
  and `glyph_tests` fails if a new symbol is not covered by the fonts.
* **A failed probe is remembered.** `ensure_probes` used to re-spawn `ffprobe`
  for any file it could not read, every frame, forever — and `start_run` refused
  to start while anything was "still probing", so one corrupt file blocked the
  whole batch. Failures now go into `probe_failed` (cleared by a rescan), the
  preview says the file is unreadable, and the batch skips it with an error.
* **Windows niceties**: `CREATE_NO_WINDOW` on every child process (no console
  flashes), CJK system font loaded when available (Chinese file names), and the
  task list can reveal an output folder in Explorer.
* **UI screenshots are rendered offscreen, not captured from a window.**
  `--screenshot` runs the real `FflowApp` against an offscreen egui context and
  rasterises the tessellated meshes in software. Two OS-level approaches were
  tried first and both are dead ends in a disconnected RDP session:
  `Windows.Graphics.Capture` sets up cleanly (`CreateForWindow`,
  `CreateFreeThreaded`, `StartCapture` all succeed) but DWM never composes, so
  `TryGetNextFrame` returns a null frame forever; `PrintWindow` with
  `PW_RENDERFULLCONTENT` returns the window frame and title bar but leaves the
  client area black, because the content lives in an OpenGL swap chain rather
  than a GDI surface. The offscreen renderer needs neither DWM nor a GPU and
  produces the exact same pixels egui would draw, so it is what ships.

Not implemented (deliberately out of scope for this iteration): in-app ffmpeg
installation/version checks beyond a startup warning, and VAAPI auto-substitution
(see above).
