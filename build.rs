fn main() {
    embed_file_icons();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // gpui-component's own Cargo config reserves a larger Windows stack,
        // but dependency configs do not reach this application. Match the
        // component's 8 MiB reserve instead of the linker's 1 MiB default.
        println!("cargo:rustc-link-arg-bin=tty7-app=/STACK:8388608");
    }

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/favicon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/favicon.ico");
        res.set("ProductName", "ctty7");
        res.set("FileDescription", "ctty7 terminal workbench");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon: {e}");
        }
        stage_bundled_conpty();
    }
}

/// Emit `$OUT_DIR/file_icons.rs`: every vendored Symbols SVG as a
/// `("files/rust.svg", include_bytes!(..))` pair, sorted by key so
/// `ui::file_icons` can binary-search it.
///
/// Generated rather than hand-listed because the set is the upstream theme's
/// (350-odd files, replaced wholesale by `scripts/sync-symbols-icons.py`), and
/// an SVG missing from a hand-kept table renders as nothing, silently.
fn embed_file_icons() {
    use std::fmt::Write as _;
    use std::path::PathBuf;

    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("assets/file-icons/symbols");
    println!("cargo:rerun-if-changed={}", root.display());

    let mut icons = Vec::new();
    for kind in ["files", "folders"] {
        let dir = root.join(kind);
        println!("cargo:rerun-if-changed={}", dir.display());
        for entry in std::fs::read_dir(&dir).expect("assets/file-icons/symbols is vendored") {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "svg") {
                let name = path.file_name().unwrap().to_str().unwrap().to_owned();
                icons.push((format!("{kind}/{name}"), path));
            }
        }
    }
    icons.sort();

    let mut out = String::from("pub(super) static SVGS: &[(&str, &[u8])] = &[\n");
    for (key, path) in &icons {
        writeln!(
            out,
            "    ({key:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("file_icons.rs");
    std::fs::write(dest, out).unwrap();
}

/// Put the bundled ConPTY beside cargo's output, the way the packaged app has
/// it beside `tty7-app.exe`.
///
/// `portable-pty` picks a sideloaded `conpty.dll` over the one in `kernel32`,
/// and finds it through the DLL search path — which starts at the directory of
/// the running executable, not the working directory. Without this a
/// development build silently runs on the in-box `conhost.exe` and loses the
/// `OSC 11` answer that packaged builds give (see `assets/windows/conpty`), so
/// the bug reappears only for people running from source.
///
/// Test executables live one directory deeper, so `deps/` gets a copy too.
#[cfg(windows)]
fn stage_bundled_conpty() {
    use std::path::{Path, PathBuf};

    println!("cargo:rerun-if-changed=assets/windows/conpty");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let vendored = match arch.as_str() {
        "x86_64" => "x64",
        // Every other Windows architecture is unreleased, so nothing is
        // vendored for it and the in-box conhost stays in charge.
        _ => return,
    };
    let source = PathBuf::from("assets/windows/conpty").join(vendored);

    // .../target/<triple>?/<profile>/build/<pkg>-<hash>/out
    let Some(target_dir) = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .and_then(|out| out.ancestors().nth(3).map(Path::to_path_buf))
    else {
        return;
    };

    for name in ["conpty.dll", "OpenConsole.exe"] {
        let from = source.join(name);
        for directory in [target_dir.clone(), target_dir.join("deps")] {
            if !directory.is_dir() {
                continue;
            }
            let to = directory.join(name);
            // Watching the destination as well as the source is what makes the
            // staging self-healing: cargo treats a path that no longer exists
            // as changed, so a copy that gets cleaned out of the target
            // directory comes back on the next build instead of staying gone
            // behind a cached build script.
            println!("cargo:rerun-if-changed={}", to.display());
            // Re-copying would fail while a previously built tty7 is running,
            // and the pair only changes when it is deliberately updated.
            let same = std::fs::metadata(&to)
                .ok()
                .zip(std::fs::metadata(&from).ok())
                .is_some_and(|(a, b)| a.len() == b.len());
            if same {
                continue;
            }
            if let Err(e) = std::fs::copy(&from, &to) {
                println!(
                    "cargo:warning=could not stage {}: {e} — this build will use the in-box conhost",
                    to.display()
                );
            }
        }
    }
}
