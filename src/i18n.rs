//! Tiny, dependency-free UI localisation.
//!
//! English source strings double as translation keys, so a missing entry falls
//! back to readable English instead of a raw `app.toolbar.run` key. Call sites
//! use [`tr!`], which also forwards format arguments:
//!
//! ```ignore
//! ui.label(tr!("Nothing selected"));
//! self.notify(tr!("started {} job(s)", n));
//! ```
//!
//! Adding a language means adding a variant to [`Language`], a catalogue and
//! one arm in [`t`]. Node titles/blurbs and the whole UI go through this module,
//! while workflow JSON keeps stable English ids and port names.

use std::cell::Cell;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Zh => "zh",
        }
    }

    /// Accepts the values people actually pass on the command line.
    ///
    /// Case, surrounding spaces and `_` vs `-` separators are all ignored, so
    /// `zh_CN`, `ZH-cn` and `zh` all work.
    pub fn from_code(code: &str) -> Option<Self> {
        let normalised = code.trim().to_ascii_lowercase().replace('_', "-");
        match normalised.as_str() {
            "en" | "en-us" | "en-gb" | "english" => Some(Language::En),
            "zh" | "zh-cn" | "zh-hans" | "zh-hant" | "cn" | "chinese" => Some(Language::Zh),
            _ => None,
        }
    }

    /// Label of the *other* language, used on the toolbar toggle.
    pub fn switch_label(self) -> &'static str {
        match self {
            Language::En => "中文",
            Language::Zh => "EN",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Language::En => Language::Zh,
            Language::Zh => Language::En,
        }
    }
}

thread_local! {
    static CURRENT: Cell<Language> = const { Cell::new(Language::En) };
}

pub fn current() -> Language {
    CURRENT.with(Cell::get)
}

pub fn set(language: Language) {
    CURRENT.with(|c| c.set(language));
}

/// Translate an English UI string. Unknown strings fall back to English.
pub fn t(en: &'static str) -> &'static str {
    if current() == Language::En {
        return en;
    }
    ZH.iter()
        .find_map(|(key, value)| (*key == en).then_some(*value))
        .unwrap_or(en)
}

/// Chinese text for `en`, whatever the active language is.
///
/// Search indexing needs both languages at once, so it cannot go through
/// `t()` — that one returns English whenever English is selected.
pub fn in_zh(en: &'static str) -> &'static str {
    ZH.iter()
        .find_map(|(key, value)| (*key == en).then_some(*value))
        .unwrap_or(en)
}

/// Translate a string that is only known at runtime (error messages, ffmpeg
/// output). Returns an owned `String` because the input is not `'static`.
///
/// Falls back to the input unchanged when the catalogue has no entry, so
/// unfamiliar ffmpeg diagnostics still reach the user verbatim.
pub fn translate_dynamic(en: &str) -> String {
    if current() == Language::En {
        return en.to_owned();
    }
    ZH.iter()
        .find_map(|(key, value)| (*key == en).then(|| (*value).to_owned()))
        .unwrap_or_else(|| en.to_owned())
}

/// Translate and substitute `{}` placeholders in one step.
///
/// `format!` insists on a compile-time literal, which a translated template
/// is not. A tiny positional formatter is enough for UI strings: every entry
/// uses plain `{}` (no width/precision specifiers), so the translated text can
/// be filled at runtime.
pub fn format(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    for arg in args {
        let Some((before, after)) = rest.split_once("{}") else {
            break;
        };
        out.push_str(before);
        let _ = write!(out, "{}", arg);
        rest = after;
    }
    out.push_str(rest);
    out
}

#[macro_export]
macro_rules! tr {
    ($en:literal) => {
        $crate::i18n::t($en)
    };
    ($en:literal, $($arg:expr),+ $(,)?) => {{
        let args: &[&dyn ::std::fmt::Display] =
            &[$(&$arg as &dyn ::std::fmt::Display),+];
        $crate::i18n::format($crate::i18n::t($en), args)
    }};
}

/// English → Chinese catalogue.
///
/// Kept as a flat table (rather than a `match`) so adding a translation is a
/// one-line change and duplicate keys cannot break the build.
const ZH: &[(&str, &str)] = &[
    // -- shell / toolbar ---------------------------------------------------
    ("untitled", "未命名"),
    ("Run", "运行"),
    ("Cancel", "取消"),
    ("Save", "保存"),
    ("Save as…", "另存为…"),
    ("Load", "加载"),
    ("New", "新建"),
    ("Help", "帮助"),
    ("Dark", "深色"),
    ("Light", "浅色"),
    ("Preview [on]", "预览 [开]"),
    ("Preview [off]", "预览 [关]"),
    ("Parallel", "并行"),
    ("Switch language", "切换语言"),
    ("Compile every file and run the batch", "编译每个文件并运行批处理"),
    ("Save workflow", "保存工作流"),
    ("Save workflow to a new JSON file", "将工作流另存为新的 JSON 文件"),
    ("Load workflow", "加载工作流"),
    ("New workflow from the default template", "从默认模板新建工作流"),
    ("Switch between the dark and light palettes", "在深色和浅色主题之间切换"),
    ("Show the generated ffmpeg command", "显示生成的 ffmpeg 命令"),
    ("How many files are processed at the same time", "同时处理多少个文件"),
    ("ffmpeg missing", "未找到 ffmpeg"),
    ("detecting…", "检测中…"),
    ("software encode", "软件编码"),
    ("{} files", "{} 个文件"),
    ("probe", "探测"),
    ("probe pending", "探测中"),
    ("connect video and/or audio", "接入视频和/或音频"),
    (".{} container", ".{} 容器"),
    (" and ", " 且 "),
    ("{} paths", "{} 个路径"),
    ("flip {}", "镜像 {}"),
    ("⚠ no path selected", "⚠ 未选择路径"),
    (" or ", " 或 "),
    ("{} file(s) skipped by a condition", "{} 个文件被条件跳过"),
    ("cancelling…", "正在取消…"),
    ("re-detecting encoders…", "正在重新检测编码器…"),
    ("command copied", "命令已复制"),
    // -- library / files ---------------------------------------------------
    ("NODE LIBRARY", "节点库"),
    ("search nodes…", "搜索节点…"),
    ("Click a node to add it, or drag it onto the canvas", "点击节点添加，或拖到画布上"),
    ("no card matches “{}”", "没有匹配“{}”的节点"),
    ("FILES", "文件"),
    ("rescan", "重新扫描"),
    ("Re-scan the Source node", "重新扫描源节点"),
    ("No files yet", "暂无文件"),
    ("Point the Source node at a folder", "让源节点指向一个文件夹"),
    ("probing…", "探测中…"),
    // -- inspector ---------------------------------------------------------
    ("INSPECTOR", "检查器"),
    ("Nothing selected", "未选择节点"),
    ("Click a node on the canvas to edit it", "点击画布上的节点进行编辑"),
    ("select / edit", "选择 / 编辑"),
    ("box-select many", "框选多个"),
    ("zoom the canvas", "缩放画布"),
    ("delete selection", "删除所选"),
    ("Delete", "删除"),
    ("Remove this node (Del)", "删除此节点 (Del)"),
    ("Compiled output", "编译结果"),
    ("{} nodes selected", "已选择 {} 个节点"),
    ("{} nodes", "{} 个节点"),
    ("Select a single node to edit its properties", "选择单个节点以编辑属性"),
    // -- tasks -------------------------------------------------------------
    ("queued", "排队中"),
    ("running", "运行中"),
    ("done", "已完成"),
    ("failed", "失败"),
    ("cancelled", "已取消"),
    ("TASKS", "任务"),
    ("no jobs yet", "暂无任务"),
    ("{} running", "{} 运行中"),
    ("{} done", "{} 完成"),
    ("{} failed", "{} 失败"),
    ("{} cancelled", "{} 已取消"),
    ("Clear", "清除"),
    ("{}% overall", "总进度 {}%"),
    ("Nothing running", "没有正在运行的任务"),
    ("Run compiles every file the Source node found", "运行会处理源节点找到的所有文件"),
    ("start the batch", "开始批处理"),
    ("reveal output", "打开输出目录"),
    ("FILE", "文件"),
    ("PROGRESS", "进度"),
    ("STATUS", "状态"),
    ("DETAIL", "详情"),
    ("hide", "收起"),
    ("error", "错误"),
    ("open", "打开"),
    ("Reveal the output folder", "打开输出文件夹"),
    // -- command preview ---------------------------------------------------
    ("COMMAND PREVIEW", "命令预览"),
    ("no files", "无文件"),
    ("Copy", "复制"),
    ("Copy the full command line", "复制完整命令行"),
    ("OUTPUT", "输出"),
    // -- notifications / errors -------------------------------------------
    ("probe failed: {}", "探测失败：{}"),
    ("batch finished — {} ok, {} failed, {} cancelled", "批处理完成 — 成功 {}，失败 {}，取消 {}"),
    ("added dropped path(s) to the Source node", "已将拖入路径添加到源节点"),
    ("no input file — pick a root folder first", "没有输入文件 — 请先选择根目录"),
    ("no input files — choose a root folder first", "没有输入文件 — 请先选择根目录"),
    ("still probing {} file(s), try again in a moment", "仍有 {} 个文件正在探测，请稍后再试"),
    ("nothing to run", "没有可运行的任务"),
    ("started {} job(s)", "已开始 {} 个任务"),
    ("saved {}", "已保存 {}"),
    ("save failed: {}", "保存失败：{}"),
    ("loaded {}", "已加载 {}"),
    ("load failed: {}", "加载失败：{}"),
    ("skipped — {}", "已跳过 — {}"),
    ("{} could not be read by ffprobe", "{} 无法被 ffprobe 读取"),
    ("{}: unreadable — ffprobe could not open it", "{}：无法读取 — ffprobe 打不开该文件"),
    ("connection rejected: {} » {} (type mismatch)", "连接被拒绝：{} » {}（类型不匹配）"),
    ("connection rejected: this would create a cycle", "连接被拒绝：这会形成循环"),
    ("removed invalid connection: {}", "已移除无效连接：{}"),
    // -- help --------------------------------------------------------------
    ("fflow — how it works", "fflow — 使用说明"),
    ("Pipeline", "流程"),
    ("Ports", "端口"),
    ("Shortcuts", "快捷键"),
    ("Templates", "模板"),
    ("Binaries", "程序路径"),
    (
        "Source » Demux » (video / audio chains) » Package. Every file found by the Source node is compiled into its own ffmpeg command.",
        "源文件 » 解复用 »（视频 / 音频链）» 封装输出。源节点找到的每个文件都会编译成一条独立的 ffmpeg 命令。",
    ),
    (
        "Blue = File, orange = Video, green = Audio. Connections between different port types are refused immediately.",
        "蓝色 = 文件，橙色 = 视频，绿色 = 音频。不同类型的端口连接会立即被拒绝。",
    ),
    ("• drag empty canvas: box select", "• 在空白画布上拖动：框选"),
    (
        "• Del / Backspace: delete the selected node · Esc: deselect",
        "• Del / Backspace：删除选中节点 · Esc：取消选择",
    ),
    ("• middle mouse / cmd+drag: pan · wheel: zoom", "• 中键拖动 / cmd+拖动：平移 · 滚轮：缩放"),
    ("• right click canvas: quick-add node menu", "• 右键画布：快速添加节点"),
    ("• drag a connection off its input port: disconnect", "• 从输入端口拖出连线：断开连接"),
    ("Override with FFLOW_FFMPEG / FFLOW_FFPROBE.", "可用 FFLOW_FFMPEG / FFLOW_FFPROBE 覆盖。"),
    // -- node categories ---------------------------------------------------
    ("Input", "输入"),
    ("Filters", "滤镜"),
    ("Logic", "逻辑"),
    ("Encoders", "编码器"),
    ("Output", "输出"),
    // -- node titles -------------------------------------------------------
    ("Source", "源文件"),
    ("Demux", "解复用"),
    ("Scale", "缩放"),
    ("Pad", "补边"),
    ("Crop", "裁剪"),
    ("Rotate / Flip", "旋转 / 镜像"),
    ("Frame Rate", "帧率"),
    ("Colour", "调色"),
    ("Sharpen", "锐化"),
    ("Denoise", "降噪"),
    ("Overlay", "叠加"),
    ("Volume", "音量"),
    ("Loudness", "响度"),
    ("Band Filter", "频段滤波"),
    ("Tempo", "变速"),
    ("Channels", "声道"),
    ("Resample", "重采样"),
    ("Raw Filter", "原生滤镜"),
    ("Condition", "条件分支"),
    ("Video Encoder", "视频编码器"),
    ("Audio Encoder", "音频编码器"),
    ("Package", "封装输出"),
    // -- node blurbs -------------------------------------------------------
    ("Root folder(s) / dropped files", "根目录 / 拖入的文件"),
    ("Split a file into video + audio", "把文件拆成视频和音频"),
    ("Resolution: exact / fit / fill", "分辨率：精确 / 适应 / 填充"),
    ("Letterbox to an exact canvas", "补边到指定画幅"),
    ("Cut a rectangle out of the frame", "从画面中裁出矩形"),
    ("90° steps and mirroring", "90° 旋转与镜像"),
    ("Resample the frame rate", "重采样帧率"),
    ("Brightness / contrast / saturation", "亮度 / 对比度 / 饱和度"),
    (".cube / .3dl look-up table", ".cube / .3dl 查找表"),
    ("Unsharp mask", "非锐化遮罩"),
    ("Temporal + spatial noise reduction", "时域 + 空域降噪"),
    ("Burn an image onto the video", "把图片叠加到视频"),
    ("Gain in dB", "增益（dB）"),
    ("Normalise to a LUFS target", "归一化到目标 LUFS"),
    ("High-pass / low-pass", "高通 / 低通"),
    ("Speed up / slow down audio", "音频加速 / 减速"),
    ("Mono / stereo / 5.1 layout", "单声道 / 立体声 / 5.1"),
    ("Change the sample rate", "修改采样率"),
    ("Verbatim filtergraph fragment", "原样写入 filtergraph"),
    ("Route each file to pass or fail", "按条件把文件分流到 pass / fail"),
    ("Container, flags, naming template", "容器、参数、命名模板"),
    // -- common form labels ------------------------------------------------
    ("Mode", "模式"),
    ("Width", "宽度"),
    ("Height", "高度"),
    ("X", "X"),
    ("Y", "Y"),
    ("Colour", "颜色"),
    ("Rotate", "旋转"),
    ("Flip", "镜像"),
    ("FPS", "帧率"),
    ("Brightness", "亮度"),
    ("Contrast", "对比度"),
    ("Saturation", "饱和度"),
    ("Gamma", "伽马"),
    ("File", "文件"),
    ("Video", "视频"),
    ("Audio", "音频"),
    ("Stream", "流"),
    ("Interp", "插值"),
    ("Amount", "强度"),
    ("Strength", "强度"),
    ("Image", "图片"),
    ("Gain", "增益"),
    ("Target", "目标"),
    ("True peak", "真峰值"),
    ("High-pass", "高通"),
    ("Low-pass", "低通"),
    ("Factor", "倍率"),
    ("Layout", "布局"),
    ("Sample rate", "采样率"),
    ("Filter", "滤镜"),
    ("Codec", "编码器"),
    ("Bitrate", "码率"),
    ("Preset", "预设"),
    ("Pixel format", "像素格式"),
    ("Container", "容器"),
    ("Name template", "命名模板"),
    ("Output dir", "输出目录"),
    ("Metadata", "元数据"),
    ("Extra args", "额外参数"),
    ("Name", "名称"),
    ("Match", "匹配"),
    ("Folders / files", "文件夹 / 文件"),
    ("Extensions", "扩展名"),
    ("Video tracks", "视频轨道"),
    ("Audio tracks", "音频轨道"),
    ("Subtitles", "字幕"),
    ("Recursive (walkdir)", "递归扫描"),
    ("Skip previous fflow outputs", "跳过之前的 fflow 输出"),
    ("Prefer hardware encoder", "优先使用硬件编码器"),
    ("Target bitrate instead of CRF", "使用目标码率代替 CRF"),
    ("movflags +faststart", "movflags +faststart"),
    // -- form hints --------------------------------------------------------
    ("scale to these numbers; -2 keeps the aspect ratio", "缩放到指定数值；-2 保持宽高比"),
    ("largest size inside the box, aspect preserved", "在目标框内取最大尺寸，保持宽高比"),
    ("cover the box, crop the overflow", "覆盖目标框，裁掉溢出部分"),
    ("accepts expressions, e.g. (ow-iw)/2", "支持表达式，例如 (ow-iw)/2"),
    ("accepts iw / ih expressions", "支持 iw / ih 表达式"),
    ("fractional rates work too, e.g. 23.976", "支持小数帧率，例如 23.976"),
    ("all-neutral values leave the picture untouched", "所有值保持中性时画面不变"),
    ("higher = smoother, more detail loss", "数值越大越平滑，细节损失越多"),
    (".cube / .3dl — usually placed before Sharpen", ".cube / .3dl — 通常放在锐化之前"),
    ("0 disables the filter", "0 表示关闭该滤镜"),
    ("0.5 = half speed, 2 = double", "0.5 = 半速，2 = 双倍速"),
    ("the image is added as an extra ffmpeg input", "图片会作为额外的 ffmpeg 输入"),
    ("dB; negative quietens, 0 leaves the level alone", "dB；负值降低音量，0 保持原电平"),
    ("integrated LUFS; -14 … -16 for web", "整体 LUFS；网络发布常用 -14 … -16"),
    ("Hz; 0 turns that side off", "Hz；0 表示关闭该侧滤波"),
    ("verbatim, e.g. setpts=0.5*PTS or aecho=0.8:0.9:1000", "原样写入，例如 setpts=0.5*PTS 或 aecho=0.8:0.9:1000"),
    ("comma separated, empty = every file", "逗号分隔，留空 = 所有文件"),
    ("stream copy: no re-encode", "流复制：不重新编码"),
    ("detecting encoders…", "正在检测编码器…"),
    ("encoder list unknown", "编码器列表未知"),
    ("not detected yet", "尚未检测"),
    ("no rules — every file takes the pass branch", "没有规则 — 所有文件走 pass 分支"),
    ("metadata key, e.g. artist", "元数据键，例如 artist"),
    ("Choose media files", "选择媒体文件"),
    ("Media", "媒体"),
    ("tokens: {dir} {stem} {name} {ext} {index} {rel_dir}", "占位符：{dir} {stem} {name} {ext} {index} {rel_dir}"),
    // -- select options ----------------------------------------------------
    ("exact", "精确"),
    ("fit", "适应"),
    ("fill", "填充"),
    ("none", "无"),
    ("h", "水平"),
    ("v", "垂直"),
    ("both", "双向"),
    ("nearest", "最近邻"),
    ("trilinear", "三线性"),
    ("tetrahedral", "四面体"),
    ("mono", "单声道"),
    ("stereo", "立体声"),
    ("first", "首个"),
    ("all", "全部"),
    ("any", "任意"),
    ("pick", "选择"),
    ("off", "关闭"),
    ("value", "值"),
    ("key", "键"),
    ("+ rule", "+ 规则"),
    ("+ Folder", "+ 文件夹"),
    ("+ Files", "+ 文件"),
    ("+ metadata", "+ 元数据"),
    ("🔍 Probe first file", "🔍 探测首个文件"),
    ("↻ detect encoders", "↻ 重新检测编码器"),
    ("  ordinals", "  序号"),
    (
        "checks the file arriving on `in`: matches leave through `pass`, the rest through `fail`. Leave a side unconnected to skip those files.",
        "检查进入 `in` 的文件：匹配的从 `pass` 离开，其余从 `fail` 离开。留空的一侧会跳过这些文件。",
    ),
    (
        "Probe a file to see its tracks. Without a probe, track lists fall back to plain ordinals.",
        "探测文件后可看到轨道。未探测时，轨道列表会退回到手动填写序号。",
    ),
    (
        "Subtitles are copied as-is (mov_text for mp4/mov). Containers without subtitle support ignore this.",
        "字幕会原样复制（mp4/mov 使用 mov_text）。不支持字幕的容器会忽略此项。",
    ),
    (
        "Substitutes a verified hardware encoder (NVENC / QSV / AMF / VideoToolbox) for the chosen software codec. Falls back when the GPU cannot encode.",
        "用已验证的硬件编码器（NVENC / QSV / AMF / VideoToolbox）替换所选软件编码器；GPU 无法编码时自动回退。",
    ),
    ("› runs as {}", "› 实际使用 {}"),
    ("⚠ {} is not available in this ffmpeg build", "⚠ 当前 ffmpeg 不提供 {}"),
    ("no video", "无视频"),
    ("no audio", "无音频"),
    ("copy", "流复制"),
    ("empty", "空"),
    ("neutral", "中性"),
    ("passthrough", "直通"),
    ("always pass", "始终通过"),
    ("tag", "标签"),
    ("file name", "文件名"),
    ("extension", "扩展名"),
    ("full path", "完整路径"),
    ("size (MB)", "大小 (MB)"),
    ("duration (s)", "时长 (秒)"),
    ("width", "宽度"),
    ("height", "高度"),
    ("fps", "帧率"),
    ("video codec", "视频编码"),
    ("audio codec", "音频编码"),
    ("bitrate (kbps)", "码率 (kbps)"),
    ("has video", "有视频"),
    ("has audio", "有音频"),
    ("metadata", "元数据"),
    ("contains", "包含"),
    ("does not contain", "不包含"),
    ("starts with", "开头是"),
    ("ends with", "结尾是"),
    ("is", "等于"),
    ("matches", "匹配"),
    // -- validation --------------------------------------------------------
    ("Select a root folder or drop files onto the canvas.", "选择根目录，或把文件拖到画布上。"),
    ("Path does not exist: {}", "路径不存在：{}"),
    ("A metadata rule needs a key (e.g. comment, artist).", "元数据规则需要一个键（例如 comment、artist）。"),
    ("Name template is empty.", "命名模板为空。"),
    (
        "scale needs at least a width or a height",
        "缩放至少需要宽度或高度",
    ),
    (
        "fill needs both a width and a height",
        "填充模式需要同时给出宽度和高度",
    ),
    ("pad needs a width and a height", "补边需要宽度和高度"),
    ("crop needs a width and a height", "裁剪需要宽度和高度"),
    ("fps must be a number", "帧率必须是数字"),
    ("fps out of range (0, 240]", "帧率超出范围 (0, 240]"),
    ("brightness out of range [-1, 1]", "亮度超出范围 [-1, 1]"),
    ("contrast out of range [0, 4]", "对比度超出范围 [0, 4]"),
    ("saturation out of range [0, 3]", "饱和度超出范围 [0, 3]"),
    ("gamma out of range [0.1, 10]", "伽马超出范围 [0.1, 10]"),
    ("pick a .cube (or .3dl) file", "请选择一个 .cube（或 .3dl）文件"),
    ("sharpen amount out of range [0, 3]", "锐化强度超出范围 [0, 3]"),
    ("denoise strength out of range [0, 20]", "降噪强度超出范围 [0, 20]"),
    ("pick an image to overlay", "请选择要叠加的图片"),
    ("overlay scale out of range [0.05, 4]", "叠加缩放超出范围 [0.05, 4]"),
    ("gain out of range [-60, 24] dB", "增益超出范围 [-60, 24] dB"),
    (
        "loudness target out of range [-70, -5] LUFS",
        "响度目标超出范围 [-70, -5] LUFS",
    ),
    (
        "true peak out of range [-9, 0] dBTP",
        "真峰值超出范围 [-9, 0] dBTP",
    ),
    ("tempo factor out of range [0.5, 2]", "变速倍数超出范围 [0.5, 2]"),
    (
        "sample rate out of range [8000, 192000]",
        "采样率超出范围 [8000, 192000]",
    ),
    (
        "a raw filter cannot contain `;` (that would start a new graph segment)",
        "原始滤镜不能包含 `;`（那会开启新的滤镜链段）",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes_round_trip() {
        for language in [Language::En, Language::Zh] {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::from_code("zh-CN"), Some(Language::Zh));
        assert_eq!(Language::from_code("EN_us"), Some(Language::En));
        assert_eq!(Language::from_code("fr"), None);
    }

    #[test]
    fn chinese_catalogue_translates_ui_and_nodes() {
        set(Language::Zh);
        assert_eq!(t("Run"), "运行");
        assert_eq!(t("Video Encoder"), "视频编码器");
        assert_eq!(t("not a key"), "not a key");
        set(Language::En);
        assert_eq!(t("Run"), "Run");
    }
}
