# fflow

**用连线搭出 ffmpeg 批处理流水线。** 纯 Rust + egui 桌面应用：左边拖节点、中间连线、右边改参数，按 Run 就把整个文件夹按同一套滤镜链编码完。

- 单个原生可执行文件，不依赖浏览器或 Node
- 一次配置，批量处理整棵目录树
- 生成的命令行随时可见、可复制，手动核对或改跑都行

---

## 功能

| | |
|---|---|
| **批量扫描** | Source 节点指向一个或多个文件夹，递归发现媒体文件，按扩展名过滤，默认跳过自己产出的文件 |
| **双轨处理** | Demux 把文件拆成视频 / 音频两条支路，各自串联滤镜，最后在 Package 汇合 |
| **17 个滤镜卡片** | 缩放、补边、裁剪、旋转、帧率、调色、3D LUT、锐化、降噪、叠加、音量、响度、频段、变速、声道、重采样、原生滤镜片段 |
| **条件分支** | Condition 按文件名 / 扩展名 / 尺寸 / 时长 / 编码 / 元数据等字段把每个文件分流到 pass 或 fail 支路 |
| **流向预览** | 点一个文件就能在画布上看到它实际经过的节点与连线，其余变暗；不运行 ffmpeg 也能验证接线的对错 |
| **硬件编码** | 启动时探测 ffmpeg 可用编码器，优先使用可用的硬件编码（如 `h264_nvenc`），失败可回退软件 |
| **实时进度** | 每个任务显示百分比、速度、已用 / 总时长；错误输出可展开、可复制 |
| **工作流文件** | 存成可读 JSON（含节点坐标），换机器打开布局完全一致 |
| **中英双语** | 右上角一键切换，选择持久化；节点库搜索同时索引两种语言 |

---

## 运行要求

- Rust 1.76+（edition 2021）
- 系统 PATH 里有 `ffmpeg` 和 `ffprobe`，或用环境变量指定：

```powershell
$env:FFLOW_FFMPEG  = "D:\tools\ffmpeg.exe"
$env:FFLOW_FFPROBE = "D:\tools\ffprobe.exe"
```

> 没有 ffmpeg 时界面仍可打开和编辑，但无法探测文件或执行批处理。

---

## 构建与运行

```powershell
cargo run --release          # 启动 GUI
cargo test                   # 103 个测试
cargo build --release        # 产物：target\release\fflow.exe
```

### 应用图标

图标由 `tools/make_icon.py` 生成，**纯 Python、零依赖**：有符号距离场按尺寸逐张光栅化（不是拿 512 缩下来），PNG 和 ICO 的字节也是手写的。

```powershell
python tools/make_icon.py    # 重写 assets/icon.svg、icon-<size>.png、icon.ico
```

图案用的是应用自己的语言——一根蓝色的 File 连线在解复用处**分叉**成橙色（视频）和绿色（音频），每条线末端一个端口圆点，和画布上的连线同色同形。16/24 px 会自动去掉端口圆点并把图形略微放大：小尺寸下圆点会糊成和连线一体的团块。

| 文件 | 用途 |
|---|---|
| `assets/icon.svg` | 矢量原件，改图形从这里改 |
| `assets/icon-16…512.png` | 逐尺寸渲染结果 |
| `assets/icon.ico` | 16–256 多尺寸，给 Windows 资源用 |
| `assets/fflow.rc` | 资源脚本，`build.rs` 交给 SDK 的 `rc.exe` 编译 |

两条通道各管一半：`icon-128.png` 被 `include_bytes!` 编进二进制，作为**窗口图标**（`with_icon`）；`icon.ico` 由 `build.rs` 链接进 `.exe`，让**资源管理器 / 任务栏 / Alt-Tab** 看到同一个图案。找不到 Windows SDK 时 `build.rs` 只打一条 `cargo:warning` 就跳过——没有图标是外观问题，编不过不是。

### 命令行辅助模式

```text
fflow                                  启动 GUI
fflow --selftest <dir>                 无界面端到端自检（真实调用 ffmpeg）
fflow --screenshot <out.ppm> [--size 1600x980] [--select N]
                             [--dir <folder>] [--probe <file>]
                             [--workflow <file>] [--zoom 1.5]
                             [--no-detect] [--demo-tasks] [--light]
                             [--flow [N]] [--lang en|zh]
                                       离屏渲染整窗 UI（不需要显示器 / GPU）
```

`--lang en|zh` 可加在任何模式前（包括 `--screenshot` 和 `--selftest`），环境变量
`FFLOW_LANG` 等价。`--screenshot` 是 `shots/` 里所有截图的产生方式，也是改完视觉后最快的验收手段。

### 接线时就会拒绝的连线

编辑器不允许画出**编不出来的图**。`compiler::wiring_problems` 只用图结构就能判定的两类错误，在连线落下的那一刻就被拒绝
（提示出现在左下角），而不是等到 Run 才报错：

| 规则 | 为什么 |
|---|---|
| 一个封装输出节点的入口只能来自**同一个解复用节点** | 一条 ffmpeg 命令只有一个 `-i`，两个解复用意味着两个文件 |
| 一个多入口端口（编码器 `in`、封装 `video`/`audio`）接多根线时，必须来自**同一个条件分支的 pass / fail 两侧** | 否则两条支路会同时生效，编译器无法决定用哪条 |

条件分支本身不受影响：`pass` 和 `fail` 各接一条进同一个入口是合法的，因为对任何文件只会有一条生效。
判断时会一路向上收集经过的条件及其出口端口，所以「条件 → 解复用 → 支路」这种写法也算数——解复用上面的条件同样约束这条支路。

已经存在于旧 JSON 里的错误接线仍能打开和编辑：只有**新增**的问题才拒绝连线，否则一个坏文件会把人锁死。
剩下的编译期报错都依赖节点设置而不是接线（两个封装写同一个路径、滤镜链没有编码器、编码器在这个 ffmpeg 里不存在……），
它们会出现在命令预览和流向浮层里。

### 流向预览

在左下角文件列表里点一个文件，画布立刻回答「这个文件会走到哪」：**不经过的节点和连线全部变暗**，
左上角的浮层列出每个 Package 的结论——接收它的写明「源文件 › 条件分支 › 解复用 › … › 封装输出」，
没接收的直接说明是哪个 Condition 把它挡掉了。整个过程不调用 ffmpeg，改一次连线就能看一次。

要点：

- 每个输出的结论就是 `compile_chain` 本身——Run 调用的同一个函数，所以预览不可能比实际多写或少写一个文件；
  高亮走的那遍遍历只负责回答「经过哪些节点」。
- 图本身编不出来时（例如跨分支接线），浮层用红点标出**编译错误原文**，并且不做变暗：那不是「这个文件没人要」，
  而是「这个图跑不了」，两者不能混为一谈。
- 条件分支（Condition 在 Demux **前面**）也会被点亮：文件确实从那个节点穿过。
- `Esc` 或浮层右上角的 ✕ 退出；点浮层里的某一行会选中对应的 Package 节点。
- 文件还在探测中时不显示预览——Condition 依赖真实时长 / 流布局，用猜测画出来的流向比不画更糟。

`--screenshot --flow [N]` 可以在离屏渲染里直接打开第 N 个文件的流向，用于回归截图。

### 多语言

界面中英双语，右上角的 `中文` / `EN` 按钮一键切换，选择写进 `settings.json` 的
`language` 字段，重启后保持；画布上已有的卡片会立即重命名，不需要重新加载工作流。

实现是「英文即键」：`src/i18n.rs` 里一张 `(英文, 中文)` 平表，源码直接写英文原文，
`tr!("Run")` 查表，查不到就原样显示英文。因此新增文案只需在表里补一行，漏翻译也不会崩，
只会显示英文。带占位符的句子用 `tr!("{} files", n)`——`format!` 要求编译期字面量，
所以 `i18n::format` 自己做了一次位置替换。

两条容易被忽略的约定：

- 节点标题、端口名、条件字段等**可序列化**的名字必须走 `tr!`；写进工作流 JSON 的只有
  `NodeKind::id()` 和端口名（`in` / `out` / `video` / `audio`），所以翻译标题不会破坏旧文件。
- 节点库搜索同时索引中英文，用中文界面搜 `resize`、或用英文界面搜 `缩放` 都能命中。

---

## 使用

1. **放 Source**：从左侧库点一下，或拖到画布；在右侧填根目录。
2. **接 Demux**：文件端口 → `in`，分出 `video` / `audio`。
3. **串滤镜**：把卡片拖到对应支路中间，端口类型不匹配会被立即拒绝。
4. **收口 Package**：视频 / 音频接进 Package，选容器、命名模板。
5. **按 Run**：底部任务区显示每文件进度；Cancel 会终止子进程并删除半成品。

### 快捷键

| 操作 | 键 |
|---|---|
| 框选 | 在空白画布上拖拽 |
| 删除选中节点 | `Del` / `Backspace` |
| 取消选择 / 退出流向预览 | `Esc` |
| 查看某文件的流向 | 点文件列表里的文件 |
| 平移画布 | 中键拖动 |
| 缩放 | 滚轮 |
| 断开连线 | 从输入端口向外拖 |

---

## 节点参考

**Input**

| 节点 | 作用 | 端口 |
|---|---|---|
| Source | 根目录 / 拖入的文件 | `file` → |
| Demux | 拆成视频 + 音频 | `in` ← `video` `audio` → |

**Filters（视频）**

| 节点 | 作用 |
|---|---|
| Scale | 精确 / 适应 / 填满 三种缩放模式 |
| Pad | 补边到指定画幅 |
| Crop | 裁出矩形区域 |
| Rotate / Flip | 90° 旋转与镜像 |
| Frame Rate | 重采样帧率 |
| Colour | 亮度 / 对比度 / 饱和度 / 伽马 |
| 3D LUT | `.cube` / `.3dl` 查找表 |
| Sharpen | 非锐化遮罩 |
| Denoise | 时域 + 空域降噪 |
| Overlay | 烧录图片（水印 / Logo） |

**Filters（音频）**

| 节点 | 作用 |
|---|---|
| Volume | 增益（dB） |
| Loudness | 归一到 LUFS 目标 |
| Band Filter | 高通 / 低通 |
| Tempo | 变速（保持音高可选） |
| Channels | 单声道 / 立体声 / 5.1 布局 |
| Resample | 采样率 |

**Logic / Encoders / Output**

| 节点 | 作用 |
|---|---|
| Raw Filter | 直接写一段 filtergraph |
| Condition | 按字段把文件分流到 pass / fail |
| Video Encoder | libx264 / libx265 / vp9 / 硬件编码器…… |
| Audio Encoder | aac / libopus / mp3…… |
| Package | 容器、flags、输出命名模板 |

编码器与 Package 的输入端是“多入”端口，编译器会为每个文件解析出唯一存活的分支。

**可以有多个 Package。** 每个没有向下游接线的 Package 就是一个输出：一个文件可以同时产出 `_fflow.mp4` 和 `_fflow.wav`，各跑一次 ffmpeg（代价是源文件解码两遍）。Condition 也可以放在 Demux **之前**，用来按文件分流整条支路——这个文件不满足条件时整条支路跳过，不会白跑一次编码。

---

## 工作流 JSON

```json
{
  "version": "1.0",
  "name": "untitled",
  "nodes": [
    { "id": 0, "kind": "source", "config": { "paths": ["D:/vids"] }, "pos": [40.0, 90.0] },
    { "id": 1, "kind": "demux",  "config": {} }
  ],
  "edges": [
    { "from": [0, "file"], "to": [1, "in"] }
  ]
}
```

- `kind` 对应 `NodeKind::id()`；`config` 是该节点的私有结构，格式不保证兼容。
- `pos` 只影响布局，可省略。
- 旧文件缺少新字段时仍可打开（`serde(default)` + `legacy.rs` 迁移）。

---

## 视觉设计规范

界面刻意做成“灰色分层 + 单一强调色”，规则都收敛在 `src/theme.rs`：

**层级（Surface）**

| 层 | 深色 | 浅色 |
|---|---|---|
| 画布 `bg` | `#08080A` | `#EFEFF2` |
| 面板 `panel` | `#101013` | `#FFFFFF` |
| 控件 `surface` | `#19191D` | `#F4F4F6` |
| 节点卡片 `card` | `#1E1E22` | `#FFFFFF` |
| 节点标题栏 `card_top` | `#26262C` | `#F7F7F9` |
| 代码预览 `code_bg` | `#0D0D11` | `#F7F7FA` |

**颜色只用在小面积上**：库/检查器里的线框图标、端口圆点、状态药丸。大面积色块全部取消，卡片也不再有顶部色条——分支靠端口圆点和连线区分。

**8 px 栅格**：`GAP = 8`；面板内边距 `12/8`，按钮高 `28`，圆角 `6`（卡片 `8`），图标 `16` 设计框。连线宽度是 `theme::WIRE`（2.5），由 `set_wire_width` 推给 vendored 编辑器——想调粗细改这一个常量，不用进 `vendor/`。

**字号阶梯**：`10 · 11 · 12 · 13 · 14 · 17`，不再出现 9.5 / 12.5 之类的散值。

**图标**：`theme::Icon` 用矢量线条手绘（16×16 网格、1.5 px 描边、统一圆角），不再使用 emoji——egui 自带字体没有那些字形，之前会渲染成豆腐块。

**层级与阴影**：节点卡片带 5 层堆叠的柔和投影 + 1 px 半透明描边，浮在点阵画布之上。

**命令预览配色**：独立 `code_*` 调色板，深色模式下每个 token 对 `code_bg` 的对比度均 ≥ 4.5:1。

---

## 源码结构

```text
.github/workflows/ ci.yml（三平台测试）、release.yml（打 tag 出四个产物）
build.rs           Windows 图标资源：rc.exe 编译 assets/fflow.rc，结果链接进 exe
assets/            icon.svg（原件）、icon-16…512.png、icon.ico、fflow.rc
tools/
└── make_icon.py   零依赖图标生成器（SDF 逐尺寸光栅化 + 手写 PNG / ICO）
src/
├── main.rs        eframe 启动、窗口图标、--selftest / --screenshot 入口
├── app.rs         应用状态、三栏布局、事件泵、拖放、无头测试
├── theme.rs       调色板、字号/间距、矢量图标、公共控件
├── i18n.rs        中英文案表、tr! 宏、运行时占位符替换
├── graph/
│   ├── mod.rs     egui_node_graph2 trait 实现、工作流 ⇄ 编辑器转换
│   ├── model.rs   Workflow / WfNode / Edge / Settings（serde）
│   ├── legacy.rs  旧格式迁移
│   ├── nodes.rs   节点种类表、编码器、条件、打包
│   └── nodes/filters.rs  各滤镜卡片
├── source.rs      rfd 对话框、目录递归、扩展名过滤
├── probe.rs       ffprobe JSON → ProbeInfo
├── encoders.rs    ffmpeg -encoders 解析、硬件探测与缓存
├── compiler.rs    图 → ffmpeg 参数（含 filter_complex）
├── runner.rs      tokio 批处理、信号量、-progress 解析、取消
└── shot.rs        离屏渲染 → 软件光栅化 → PPM
```

线程模型：UI 线程只绘制与读通道。探测和 ffmpeg 进程跑在 2 线程 tokio 运行时，结果走无界通道，每帧 `try_recv` 排空，界面永不阻塞。

### 关于 `vendor/egui_node_graph2`

`egui_node_graph2` 0.7 是 crates.io 上唯一的版本，且已停止更新。为了让节点的缩放、连线和卡片外观符合上面这套规范，仓库内置了它的源码副本并在 `Cargo.toml` 里用 `[patch.crates-io]` 指向它。每处改动都以 `fflow patch:` 注释标记，集中在 `editor_ui.rs`：

- 卡片边距缩放、内边距与行距按 zoom 缩放（修掉缩小时卡片被撑高的老 bug）
- 滚轮缩放读取原始 delta，而不是平滑后的尾部，避免停下后继续微缩放导致文字抖动
- 关闭按钮的 ✕ 描边随 zoom 缩放，不再在小卡片上显得又粗又大
- 卡片使用主题色 `faint_bg_color` / `text_color`，而不是硬编码的 `#3f3f3f`
- 连接线是单层贝塞尔（无辉光），宽度由 `theme::WIRE` 通过 `set_wire_width` 注入，并画在卡片下方
- 连线层可以整层「调暗 / 加重」：`set_flow_highlight` 传一张连线表，表内的保持原色并加粗 1.35 倍，其余降低 alpha。
  连线画在卡片**后面**，宿主没法事后覆盖，所以流向预览只能从这里入手；调用方在 `draw_graph_editor`
  之后立刻传 `None`，高亮不会跨帧残留
- 多入端口（编码器 `in`、Package 的 `video`/`audio`）不再画成一整条圆角色块，而是每条连线一个圆点、卡片按行增高——和 Package 多一个端口时一样地“长高”，单条连线时与普通端口完全一致
- 卡片 8 px 圆角、主题发丝描边、柔和投影
- 卡片矩形通过 `GraphResponse::node_rects` 暴露给应用（选中环、重绘的关闭按钮都依赖它）
- 移除右键弹出的 node finder（左侧库已经覆盖该功能）

节点内的 `probe` 按钮由 `theme::mini_button` 绘制，尺寸、字号、圆角和描边都会跟随 `theme::ui_zoom`，不会再在缩小时保持固定像素尺寸。

### 已知限制

- 画布缩放到 35% 以下时，节点正文只有几个像素高，文字不再具备可读性；需要看清内容时请放大。
- **滚动区域最后一行的渐隐**是手绘网格渐变，不是真正的模糊。
- 画布点阵随缩放变化密度，不严格对齐图坐标。

---

## 发布

跨平台靠的是代码结构，不是祈祷。所有跟系统打交道的地方都是收敛的：

| 关注点 | 处理 |
|---|---|
| 子进程 | 一律 `Command::new("ffmpeg")` 走 PATH，没有 FFI、没有 libav 链接；`CREATE_NO_WINDOW` 只在 `#[cfg(windows)]` 里出现 |
| 打开输出目录 | `explorer` / `open` / `xdg-open` 三分支 `cfg` |
| 字体 | 三平台路径列表 + egui 内置 Ubuntu/Hack 兜底，CJK 作为 fallback 追加 |
| 图标 | `build.rs` 在非 Windows 上直接 return；窗口图标走 `with_icon`（Wayland 会忽略，符合预期） |
| 离屏渲染 | `--screenshot` 用裸 `egui::Context` + 软件光栅化，不建窗口、不碰 GL，所以无头机器也能跑 |
| 构建期系统库 | 一个都不需要：winit 默认开 `wayland-dlopen` 与 `xkbcommon-dl`，`x11-dl` 的 pkg-config 只是软探测，rfd 0.15 走 xdg-desktop-portal 而不是 GTK |

两个 workflow：

| 文件 | 触发 | 做什么 |
|---|---|---|
| `.github/workflows/ci.yml` | push / PR | 三个系统各跑一遍 `cargo test` |
| `.github/workflows/release.yml` | push tag `v*` | 编四个目标、逐个冒烟测试、打包、挂到 Release |

发版就是打 tag，一次 tag 对应一次运行、一套产物：

```powershell
git tag v0.1.0
git push origin v0.1.0
```

产物四个：`fflow-<版本>-windows-x64.zip`、`-linux-x64.tar.gz`、`-macos-arm64.tar.gz`、`-macos-x64.tar.gz`。

每个平台编完会跑一次 `fflow --screenshot`，检查输出是 PPM 的 `P6` 头。编译通过只说明链上了，渲染出一帧才说明二进制真的能启动——字体、资源、图标都在里面。

几个刻意的选择：

- **`ubuntu-22.04` 而不是 `-latest`**：glibc 向下兼容，在哪个系统上编就决定了最低能跑哪个系统。用 24.04 编出来的二进制跑不了 Ubuntu 22.04 / Debian 12。
- **macOS 两个架构都在 arm64 runner 上编**：Apple SDK 自带两份 slice，不需要单独的 Intel 机器。
- **`release` job `needs: build`**：四个平台全过才发版。少一个 macOS 包的 release 比一次红的 CI 更难发现。
- **`workflow_dispatch` 只编不发**：没有 tag 时 release job 直接跳过，可以拿它干跑。
- **tag 与 `Cargo.toml` 版本号比对**：`v0.2.0` 打在 0.1.0 的树上会让产物名全是错的版本，下游不会有人发现。

---

## 验证

```powershell
cargo test                                  # 103 个单元 + 无头 UI 测试
cargo run -- --selftest testdata            # 真实调用 ffmpeg 的端到端检查
cargo run -- --screenshot out.ppm           # 整窗离屏渲染
cargo run -- --screenshot zh.ppm --lang zh  # 中文界面验收（含 CJK 字形）
cargo run -- --screenshot flow.ppm --flow 0 # 流向预览（第 0 个文件）
```

无头测试用 `egui::Context::run` 直接驱动 `draw_all`，因此布局、点击、框选、缩放、拖放都能在没有窗口和 GPU 的机器上回归。

---

## 许可

本项目代码为 MIT。`vendor/egui_node_graph2` 沿用其上游 MIT 许可；ffmpeg 为独立程序，按其自身许可使用。
