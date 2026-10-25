//! Turning a Source node into a concrete file list.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Extensions offered in the native file dialog.
pub const EXT_LIST: &[&str] = &[
    "mp4", "mkv", "mov", "m4v", "avi", "webm", "ts", "flv", "wmv", "mp3", "m4a", "aac", "wav",
    "flac", "ogg", "opus", "wma",
];

pub fn short_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub fn parse_extensions(spec: &str) -> Vec<String> {
    spec.split(|c| c == ',' || c == ';' || c == ' ')
        .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn has_allowed_ext(path: &Path, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    path.extension()
        .map(|e| {
            let e = e.to_string_lossy().to_ascii_lowercase();
            allowed.iter().any(|a| *a == e)
        })
        .unwrap_or(false)
}

/// fflow writes its results next to the sources by default; those must not be
/// picked up again as inputs on the next run.
pub fn looks_like_output(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if stem.ends_with("_fflow") {
        return true;
    }
    path.parent()
        .and_then(|p| p.file_name())
        .map(|d| d.to_string_lossy().eq_ignore_ascii_case("fflow_out"))
        .unwrap_or(false)
}

/// Expand a list of paths (directories and/or files) into media files.
///
/// * directories are walked with `walkdir`, honouring `recursive`
/// * extension filter is applied to every candidate
/// * previous fflow outputs are skipped unless `skip_own_outputs` is false
/// * result is sorted and de-duplicated so batch order is deterministic
pub fn expand(
    paths: &[String],
    recursive: bool,
    extensions: &str,
    skip_own_outputs: bool,
) -> Vec<PathBuf> {
    let allowed = parse_extensions(extensions);
    let mut out = BTreeSet::new();

    for p in paths {
        let path = Path::new(p);
        if path.is_file() {
            if has_allowed_ext(path, &allowed) {
                out.insert(path.to_path_buf());
            }
        } else if path.is_dir() {
            let walker = WalkDir::new(path).max_depth(if recursive { usize::MAX } else { 1 });
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let p = entry.into_path();
                if has_allowed_ext(&p, &allowed) && !(skip_own_outputs && looks_like_output(&p)) {
                    out.insert(p);
                }
            }
        }
    }

    out.into_iter().collect()
}

pub fn pick_folder(start: Option<&Path>) -> Option<PathBuf> {
    let mut dlg = rfd::FileDialog::new().set_title("Choose the root folder");
    if let Some(s) = start {
        if s.is_dir() {
            dlg = dlg.set_directory(s);
        }
    }
    dlg.pick_folder()
}

pub fn pick_workflow_save(start: Option<&Path>) -> Option<PathBuf> {
    let mut dlg = rfd::FileDialog::new()
        .set_title("Save workflow")
        .add_filter("fflow workflow", &["json"])
        .set_file_name("workflow.fflow.json");
    if let Some(s) = start {
        dlg = dlg.set_directory(s);
    }
    dlg.save_file()
}

pub fn pick_workflow_open(start: Option<&Path>) -> Option<PathBuf> {
    let mut dlg = rfd::FileDialog::new()
        .set_title("Load workflow")
        .add_filter("fflow workflow", &["json"]);
    if let Some(s) = start {
        dlg = dlg.set_directory(s);
    }
    dlg.pick_file()
}
