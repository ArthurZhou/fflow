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
cargo test                   # 89 个测试
cargo build --release        # 产物：target\release\fflow.exe
```

### 命令行辅助模式

```text
fflow                                  启动 GUI
fflow --selftest <dir>                 无界面端到端自检（真实调用 ffmpeg）
fflow --screenshot <out.ppm> [--size 1600x980] [--select N]
                             [--dir <folder>] [--probe <file>]
                             [--workflow <file>] [--zoom 1.5]
                             [--no-detect] [--demo-tasks] [--light]
                             [--lang en|zh]
                                       离屏渲染整窗 UI（不需要显示器 / GPU）
```

`--lang en|zh` 可加在任何模式前（包括 `--screenshot` 和 `--selftest`），环境变量
`FFLOW_LANG` 等价。`--screenshot` 是 `shots/` 里所有截图的产生方式，也是改完视觉后最快的验收手段。

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
| 取消选择 | `Esc` |
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
src/
├── main.rs        eframe 启动、--selftest / --screenshot 入口
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
- 卡片 8 px 圆角、主题发丝描边、柔和投影
- 卡片矩形通过 `GraphResponse::node_rects` 暴露给应用（选中环、重绘的关闭按钮都依赖它）
- 移除右键弹出的 node finder（左侧库已经覆盖该功能）

节点内的 `probe` 按钮由 `theme::mini_button` 绘制，尺寸、字号、圆角和描边都会跟随 `theme::ui_zoom`，不会再在缩小时保持固定像素尺寸。

### 已知限制

- 画布缩放到 35% 以下时，节点正文只有几个像素高，文字不再具备可读性；需要看清内容时请放大。
- **滚动区域最后一行的渐隐**是手绘网格渐变，不是真正的模糊。
- 画布点阵随缩放变化密度，不严格对齐图坐标。

---

## 验证

```powershell
cargo test                                  # 89 个单元 + 无头 UI 测试
cargo run -- --selftest testdata            # 真实调用 ffmpeg 的端到端检查
cargo run -- --screenshot out.ppm           # 整窗离屏渲染
cargo run -- --screenshot zh.ppm --lang zh  # 中文界面验收（含 CJK 字形）
```

无头测试用 `egui::Context::run` 直接驱动 `draw_all`，因此布局、点击、框选、缩放、拖放都能在没有窗口和 GPU 的机器上回归。

---

## 许可

本项目代码为 MIT。`vendor/egui_node_graph2` 沿用其上游 MIT 许可；ffmpeg 为独立程序，按其自身许可使用。
