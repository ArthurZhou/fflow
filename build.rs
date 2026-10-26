//! Link the application icon into the Windows executable.
//!
//! Explorer, the taskbar, Alt-Tab and the window title bar read the icon out of
//! the `.exe` itself — it is not something the process can hand over at run
//! time — so the `.ico` has to be compiled into a resource and passed to the
//! linker. Cargo has no built-in way to do that, hence this script.
//!
//! A missing Windows SDK is never fatal: the build succeeds and the binary
//! keeps the default icon, which beats failing to build over a picture.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/fflow.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // The `.res` is a link.exe input. A GNU build would need `windres` and a
    // `.o` instead; that path is not wired up because nothing here ships it.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!("cargo:warning=fflow icon: only the MSVC toolchain embeds it, skipping");
        return;
    }
    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=fflow icon: rc.exe not found — install the Windows SDK \
             (or set FFLOW_RC) to embed it"
        );
        return;
    };

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is always set"));
    let res = out.join("fflow.res");
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/I")
        .arg("assets")
        .arg("/fo")
        .arg(&res)
        .arg("assets/fflow.rc")
        .status();
    match status {
        // `-bins` keeps the resource out of the test harnesses, which would
        // otherwise each link a copy of it for nothing.
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", res.display()),
        Ok(s) => println!("cargo:warning=fflow icon: rc.exe failed ({s})"),
        Err(e) => println!(
            "cargo:warning=fflow icon: could not run {}: {e}",
            rc.display()
        ),
    }
}

/// `rc.exe` ships with the Windows SDK and is not on `PATH` for a plain shell,
/// so look through the SDK's versioned directories and take the newest.
fn find_rc() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FFLOW_RC").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(p) = on_path("rc.exe") {
        return Some(p);
    }

    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    };
    let kits = std::env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ProgramFiles").map(PathBuf::from))?
        .join("Windows Kits")
        .join("10")
        .join("bin");

    let mut found: Vec<PathBuf> = std::fs::read_dir(kits)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join(arch).join("rc.exe"))
        .filter(|p| p.is_file())
        .collect();
    // Numeric order, not lexical: "10.0.9000.0" is newer than "10.0.14393.0"
    // even though it sorts the other way round as text.
    found.sort_by_key(|p| sdk_version(p));
    found.pop()
}

/// `.../Windows Kits/10/bin/<version>/<arch>/rc.exe` → `<version>` as numbers.
fn sdk_version(rc: &Path) -> [u64; 4] {
    let mut v = [0u64; 4];
    let name = rc
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|n| n.to_str());
    for (slot, part) in v.iter_mut().zip(name.unwrap_or("").split('.')) {
        *slot = part.parse().unwrap_or(0);
    }
    v
}

fn on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
}
