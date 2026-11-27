//! Detect locally installed code editors and launch them against a pane's
//! working directory (local, SSH remote, or WSL).
//!
//! Detection touches the filesystem and `PATH`, so callers run
//! [`detect_installed`] on a background executor and cache the result on the
//! app instead of probing on every render frame.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// An external editor or the system file manager that can open a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenInApp {
    VsCode,
    Cursor,
    Windsurf,
    Zed,
    IntelliJ,
    CLion,
    RustRover,
    PyCharm,
    WebStorm,
    GoLand,
    Qoder,
    SublimeText,
    VsCodeInsiders,
    FileManager,
}

/// How an application handles non-local working directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSupport {
    /// Only understands paths on this machine.
    LocalOnly,
    /// Accepts `--remote ssh-remote+<authority> <path>` and `--remote wsl+<distro> <path>`.
    VsCodeRemote,
    /// Accepts `ssh://<authority><path>`.
    ZedSshUri,
}

impl OpenInApp {
    /// External editors in default priority order when the user has not chosen
    /// one yet (or their saved choice is unavailable for the current pane).
    pub const ALL_EDITORS: &'static [Self] = &[
        Self::VsCode,
        Self::Cursor,
        Self::Windsurf,
        Self::Zed,
        Self::IntelliJ,
        Self::CLion,
        Self::RustRover,
        Self::PyCharm,
        Self::WebStorm,
        Self::GoLand,
        Self::Qoder,
        Self::SublimeText,
        Self::VsCodeInsiders,
    ];

    /// Stable identifier persisted in `Config::open_in_editor`.
    pub const fn id(self) -> &'static str {
        match self {
            Self::VsCode => "vscode",
            Self::Cursor => "cursor",
            Self::Windsurf => "windsurf",
            Self::Zed => "zed",
            Self::IntelliJ => "intellij",
            Self::CLion => "clion",
            Self::RustRover => "rustrover",
            Self::PyCharm => "pycharm",
            Self::WebStorm => "webstorm",
            Self::GoLand => "goland",
            Self::Qoder => "qoder",
            Self::SublimeText => "sublime",
            Self::VsCodeInsiders => "vscode-insiders",
            Self::FileManager => "file-manager",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "vscode" => Some(Self::VsCode),
            "cursor" => Some(Self::Cursor),
            "windsurf" => Some(Self::Windsurf),
            "zed" => Some(Self::Zed),
            "intellij" => Some(Self::IntelliJ),
            "clion" => Some(Self::CLion),
            "rustrover" => Some(Self::RustRover),
            "pycharm" => Some(Self::PyCharm),
            "webstorm" => Some(Self::WebStorm),
            "goland" => Some(Self::GoLand),
            "qoder" => Some(Self::Qoder),
            "sublime" => Some(Self::SublimeText),
            "vscode-insiders" => Some(Self::VsCodeInsiders),
            "file-manager" => Some(Self::FileManager),
            _ => None,
        }
    }

    /// Brand display name for external editors, or `None` for
    /// [`Self::FileManager`] (localized by the UI layer).
    pub const fn brand_name(self) -> Option<&'static str> {
        match self {
            Self::VsCode => Some("VS Code"),
            Self::Cursor => Some("Cursor"),
            Self::Windsurf => Some("Windsurf"),
            Self::Zed => Some("Zed"),
            Self::IntelliJ => Some("IntelliJ IDEA"),
            Self::CLion => Some("CLion"),
            Self::RustRover => Some("RustRover"),
            Self::PyCharm => Some("PyCharm"),
            Self::WebStorm => Some("WebStorm"),
            Self::GoLand => Some("GoLand"),
            Self::Qoder => Some("Qoder"),
            Self::SublimeText => Some("Sublime Text"),
            Self::VsCodeInsiders => Some("VS Code Insiders"),
            Self::FileManager => None,
        }
    }

    pub const fn remote_support(self) -> RemoteSupport {
        match self {
            Self::VsCode | Self::VsCodeInsiders | Self::Cursor | Self::Windsurf => {
                RemoteSupport::VsCodeRemote
            }
            Self::Zed => RemoteSupport::ZedSshUri,
            Self::IntelliJ
            | Self::CLion
            | Self::RustRover
            | Self::PyCharm
            | Self::WebStorm
            | Self::GoLand
            | Self::Qoder
            | Self::SublimeText
            | Self::FileManager => RemoteSupport::LocalOnly,
        }
    }

    /// Short protocol hint shown beside the editor name in non-local menus.
    pub fn location_hint(self, location: &OpenLocation) -> Option<&'static str> {
        match (self.remote_support(), location) {
            (RemoteSupport::VsCodeRemote, OpenLocation::RemoteSsh { .. }) => Some("Remote-SSH"),
            (RemoteSupport::ZedSshUri, OpenLocation::RemoteSsh { .. }) => Some("ssh://"),
            (RemoteSupport::VsCodeRemote, OpenLocation::Wsl { .. }) => Some("WSL"),
            _ => None,
        }
    }
}

/// An external editor confirmed to exist on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedApp {
    pub app: OpenInApp,
    pub executable: PathBuf,
}

impl DetectedApp {
    /// Whether this detected installation can launch against `location`.
    pub fn supports_location(&self, location: &OpenLocation) -> bool {
        self.location_error(location).is_none()
    }

    /// Why this installation cannot open `location`, if any, derived from the
    /// same command builder used when launching.
    pub fn location_error(&self, location: &OpenLocation) -> Option<OpenInError> {
        build_launch_command(self, location).err()
    }
}

/// The directory to open, classified by how external editors can reach it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenLocation {
    Local(PathBuf),
    /// Remote host reachable via SSH; `authority` is `[user@]host[:port]`.
    RemoteSsh {
        authority: String,
        path: String,
    },
    /// Windows Subsystem for Linux distribution.
    Wsl {
        distro: String,
        path: String,
    },
    /// Remote directory without a direct SSH/WSL authority for external editors.
    RemoteUnsupported {
        path: String,
    },
}

impl OpenLocation {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    pub fn is_remote(&self) -> bool {
        !self.is_local()
    }
}

/// Structured failure returned by command construction or process spawning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenInError {
    LocalOnly(OpenInApp),
    UnsupportedLocation(OpenInApp),
    MissingMacosCli(OpenInApp),
    NotInstalled(OpenInApp),
    SpawnFailed { program: PathBuf, reason: String },
}

/// Format an SSH authority string `[user@]host[:port]` (omitting port 22 and
/// empty components, and bracketing bare IPv6 literals). Returns `None` when
/// `host` is blank.
pub fn format_ssh_authority(user: &str, host: &str, port: u16) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    let bracketed_host = if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let user = user.trim();
    let mut authority = String::new();
    if !user.is_empty() {
        authority.push_str(user);
        authority.push('@');
    }
    authority.push_str(&bracketed_host);
    if port != 0 && port != 22 {
        authority.push(':');
        authority.push_str(&port.to_string());
    }
    Some(authority)
}

/// Normalize an SSH target string (`[user@]host[:port]`) by trimming whitespace
/// and bracketing unbracketed IPv6 hosts. Returns `None` when empty or missing
/// a host component.
pub fn normalize_ssh_authority(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (user_prefix, host_part) = match trimmed.rsplit_once('@') {
        Some((user, host)) => {
            let user = user.trim();
            ((!user.is_empty()).then_some(user), host.trim())
        }
        None => (None, trimmed),
    };
    if host_part.is_empty() {
        return None;
    }
    let has_multiple_colons = host_part.split(':').nth(2).is_some();
    let normalized_host = if has_multiple_colons && !host_part.starts_with('[') {
        format!("[{host_part}]")
    } else {
        host_part.to_string()
    };
    Some(match user_prefix {
        Some(user) => format!("{user}@{normalized_host}"),
        None => normalized_host,
    })
}

/// Resolved command invocation ready to be spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Picks the primary action for the split button and global shortcut.
///
/// Respects the user's saved `preferred` app whenever it is installed and can
/// open `location`; otherwise falls back to the first installed editor that
/// supports `location`, and finally to [`OpenInApp::FileManager`] on local
/// panes.
pub fn resolve_primary(
    installed: &[DetectedApp],
    preferred: Option<OpenInApp>,
    location: &OpenLocation,
) -> Option<OpenInApp> {
    let available = |app: OpenInApp| -> bool {
        if app == OpenInApp::FileManager {
            return location.is_local();
        }
        installed
            .iter()
            .find(|d| d.app == app)
            .is_some_and(|d| d.supports_location(location))
    };

    if let Some(pref) = preferred
        && available(pref)
    {
        return Some(pref);
    }
    if let Some(found) = installed
        .iter()
        .find(|d| d.supports_location(location))
        .map(|d| d.app)
    {
        return Some(found);
    }
    location.is_local().then_some(OpenInApp::FileManager)
}

/// Launch `app` (an installed editor or [`OpenInApp::FileManager`]) against
/// `location`.
pub fn launch(
    app: OpenInApp,
    installed: &[DetectedApp],
    location: &OpenLocation,
) -> Result<(), OpenInError> {
    match (app, location) {
        (OpenInApp::FileManager, OpenLocation::Local(path)) => open_in_file_manager(path),
        (OpenInApp::FileManager, _) => Err(OpenInError::LocalOnly(OpenInApp::FileManager)),
        (editor, loc) => match installed.iter().find(|d| d.app == editor) {
            Some(detected) => launch_editor(detected, loc),
            None => Err(OpenInError::NotInstalled(editor)),
        },
    }
}

/// Strip Windows extended-length verbatim prefixes (`\\?\C:\...`, `\\?\UNC\...`)
/// so external editors and Explorer receive a path they understand.
pub fn clean_local_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn macos_open_app(bundle: &Path, arg: String) -> LaunchCommand {
    LaunchCommand {
        program: PathBuf::from("open"),
        args: vec!["-a".into(), bundle.to_string_lossy().into_owned(), arg],
    }
}

/// Build the launch command for `target` against `location`.
pub fn build_launch_command(
    target: &DetectedApp,
    location: &OpenLocation,
) -> Result<LaunchCommand, OpenInError> {
    let exe = &target.executable;

    match location {
        OpenLocation::Local(path) => {
            let path = clean_local_path(path).to_string_lossy().into_owned();
            if is_macos_app_bundle(exe) {
                return Ok(macos_open_app(exe, path));
            }
            Ok(LaunchCommand {
                program: exe.clone(),
                args: vec![path],
            })
        }
        OpenLocation::RemoteSsh { authority, path } => {
            let authority = authority.trim();
            if authority.is_empty() {
                return Err(OpenInError::UnsupportedLocation(target.app));
            }
            match target.app.remote_support() {
                RemoteSupport::VsCodeRemote => {
                    if is_macos_app_bundle(exe) {
                        return Err(OpenInError::MissingMacosCli(target.app));
                    }
                    Ok(LaunchCommand {
                        program: exe.clone(),
                        args: vec![
                            "--remote".into(),
                            format!("ssh-remote+{authority}"),
                            path.clone(),
                        ],
                    })
                }
                RemoteSupport::ZedSshUri => {
                    let uri = format_zed_ssh_uri(authority, path);
                    if is_macos_app_bundle(exe) {
                        return Ok(macos_open_app(exe, uri));
                    }
                    Ok(LaunchCommand {
                        program: exe.clone(),
                        args: vec![uri],
                    })
                }
                RemoteSupport::LocalOnly => Err(OpenInError::LocalOnly(target.app)),
            }
        }
        OpenLocation::Wsl { distro, path } => {
            let distro = distro.trim();
            if distro.is_empty() {
                return Err(OpenInError::UnsupportedLocation(target.app));
            }
            match target.app.remote_support() {
                RemoteSupport::VsCodeRemote => {
                    if is_macos_app_bundle(exe) {
                        return Err(OpenInError::MissingMacosCli(target.app));
                    }
                    Ok(LaunchCommand {
                        program: exe.clone(),
                        args: vec!["--remote".into(), format!("wsl+{distro}"), path.clone()],
                    })
                }
                RemoteSupport::ZedSshUri => Err(OpenInError::UnsupportedLocation(target.app)),
                RemoteSupport::LocalOnly => Err(OpenInError::LocalOnly(target.app)),
            }
        }
        OpenLocation::RemoteUnsupported { .. } => Err(OpenInError::UnsupportedLocation(target.app)),
    }
}

fn format_zed_ssh_uri(authority: &str, path: &str) -> String {
    if path.starts_with('/') {
        format!("ssh://{authority}{path}")
    } else {
        format!("ssh://{authority}/{path}")
    }
}

fn is_macos_app_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
}

fn spawn_null_io(cmd: &mut Command, program: &Path) -> Result<(), OpenInError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| OpenInError::SpawnFailed {
            program: program.to_path_buf(),
            reason: e.to_string(),
        })
}

/// Spawn an external editor detached from Hitty.
pub fn launch_editor(target: &DetectedApp, location: &OpenLocation) -> Result<(), OpenInError> {
    let cmd = build_launch_command(target, location)?;
    spawn_detached(&cmd)
}

fn spawn_detached(launch: &LaunchCommand) -> Result<(), OpenInError> {
    let mut cmd = Command::new(&launch.program);
    cmd.args(&launch.args);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        // Windows can implicitly run batch files through cmd.exe. Directory
        // and remote authority arguments must reach a native executable.
        if !launch
            .program
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err(OpenInError::SpawnFailed {
                program: launch.program.clone(),
                reason: "expected a native Windows editor executable".into(),
            });
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    spawn_null_io(&mut cmd, &launch.program)
}

/// Open a local directory in the system file manager.
pub fn open_in_file_manager(path: &Path) -> Result<(), OpenInError> {
    let cleaned = clean_local_path(path);
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("explorer.exe");
        if cleaned.is_dir() {
            cmd.arg(&cleaned);
        } else {
            cmd.arg(format!("/select,{}", cleaned.display()));
        }
        spawn_null_io(&mut cmd, Path::new("explorer.exe"))
    }
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        if !cleaned.is_dir() {
            cmd.arg("-R");
        }
        cmd.arg(&cleaned);
        spawn_null_io(&mut cmd, Path::new("open"))
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let target = if cleaned.is_dir() {
            cleaned.as_path()
        } else {
            cleaned.parent().unwrap_or(cleaned.as_path())
        };
        let mut cmd = Command::new("xdg-open");
        cmd.arg(target);
        spawn_null_io(&mut cmd, Path::new("xdg-open"))
    }
}

/// Detect installed editors on this machine, in [`OpenInApp::ALL_EDITORS`]
/// order. Safe to run on a background thread.
pub fn detect_installed() -> Vec<DetectedApp> {
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|raw| std::env::split_paths(&raw).collect())
        .unwrap_or_default();

    OpenInApp::ALL_EDITORS
        .iter()
        .filter_map(|&app| {
            let executable = detect_one(app, &path_dirs)?;
            Some(DetectedApp { app, executable })
        })
        .collect()
}

fn find_on_path(path_dirs: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    for dir in path_dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(any(target_os = "windows", test))]
struct WindowsSearchRoots<'a> {
    path_dirs: &'a [PathBuf],
    toolbox_scripts: Option<&'a Path>,
    program_files: Option<&'a Path>,
    local_programs: Option<&'a Path>,
}

#[cfg(any(target_os = "windows", test))]
fn find_windows_editor(
    roots: &WindowsSearchRoots<'_>,
    install_dir: &str,
    executable: &str,
    launcher: &str,
) -> Option<PathBuf> {
    for dir in roots.path_dirs {
        let candidate = dir.join(executable);
        if candidate.is_file() {
            return Some(candidate);
        }
        // The PATH entry may be a bin/ directory or a resources/app/bin/
        // directory. Use the batch launcher only to locate the installation.
        if dir.join(launcher).is_file()
            && let Some(candidate) = dir
                .ancestors()
                .skip(1)
                .take(3)
                .map(|parent| parent.join(executable))
                .find(|path| path.is_file())
        {
            return Some(candidate);
        }
    }
    [roots.local_programs, roots.program_files]
        .into_iter()
        .flatten()
        .map(|root| root.join(install_dir).join(executable))
        .find(|path| path.is_file())
}

#[cfg(target_os = "windows")]
fn detect_one(app: OpenInApp, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
    let local_programs = local_app_data.as_ref().map(|d| d.join("Programs"));
    let toolbox_scripts = local_app_data
        .as_ref()
        .map(|d| d.join("JetBrains").join("Toolbox").join("scripts"));

    let roots = WindowsSearchRoots {
        path_dirs,
        toolbox_scripts: toolbox_scripts.as_deref(),
        program_files: program_files.as_deref(),
        local_programs: local_programs.as_deref(),
    };

    let check_files = |candidates: Vec<Option<PathBuf>>| -> Option<PathBuf> {
        candidates.into_iter().flatten().find(|p| p.is_file())
    };

    match app {
        OpenInApp::VsCode => {
            find_windows_editor(&roots, "Microsoft VS Code", "Code.exe", "code.cmd")
        }
        OpenInApp::VsCodeInsiders => {
            find_on_path(path_dirs, &["code-insiders.exe"]).or_else(|| {
                find_windows_editor(
                    &roots,
                    "Microsoft VS Code Insiders",
                    "Code - Insiders.exe",
                    "code-insiders.cmd",
                )
            })
        }
        OpenInApp::Cursor => find_windows_editor(&roots, "Cursor", "Cursor.exe", "cursor.cmd"),
        OpenInApp::Windsurf => {
            find_windows_editor(&roots, "Windsurf", "Windsurf.exe", "windsurf.cmd")
        }
        OpenInApp::Zed => find_windows_editor(&roots, "Zed", "Zed.exe", "zed.cmd"),
        OpenInApp::Qoder => find_windows_editor(&roots, "Qoder", "Qoder.exe", "qoder.cmd"),
        OpenInApp::SublimeText => find_on_path(path_dirs, &["subl.exe"]).or_else(|| {
            check_files(vec![
                program_files
                    .as_ref()
                    .map(|d| d.join("Sublime Text").join("subl.exe")),
                program_files
                    .as_ref()
                    .map(|d| d.join("Sublime Text 3").join("subl.exe")),
            ])
        }),
        OpenInApp::IntelliJ => detect_jetbrains_windows(&roots, "IntelliJ IDEA", "idea64.exe"),
        OpenInApp::CLion => detect_jetbrains_windows(&roots, "CLion", "clion64.exe"),
        OpenInApp::RustRover => detect_jetbrains_windows(&roots, "RustRover", "rustrover64.exe"),
        OpenInApp::PyCharm => detect_jetbrains_windows(&roots, "PyCharm", "pycharm64.exe"),
        OpenInApp::WebStorm => detect_jetbrains_windows(&roots, "WebStorm", "webstorm64.exe"),
        OpenInApp::GoLand => detect_jetbrains_windows(&roots, "GoLand", "goland64.exe"),
        OpenInApp::FileManager => None,
    }
}

#[cfg(any(target_os = "windows", test))]
fn detect_jetbrains_windows(
    roots: &WindowsSearchRoots<'_>,
    dir_prefix: &str,
    bin_exe: &str,
) -> Option<PathBuf> {
    if let Some(found) = find_on_path(roots.path_dirs, &[bin_exe]) {
        return Some(found);
    }
    if let Some(scripts) = roots.toolbox_scripts
        && let Some(found) = find_on_path(&[scripts.to_path_buf()], &[bin_exe])
    {
        return Some(found);
    }
    let jb_roots = [
        roots.program_files.map(|d| d.join("JetBrains")),
        roots.local_programs.map(|d| d.to_path_buf()),
    ];
    for root in jb_roots.into_iter().flatten() {
        if let Some(found) = find_newest_prefixed_bin(&root, dir_prefix, bin_exe) {
            return Some(found);
        }
    }
    None
}

#[cfg(any(target_os = "windows", test))]
fn find_newest_prefixed_bin(root: &Path, dir_prefix: &str, bin_exe: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut matches: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(dir_prefix))
        })
        .map(|e| e.path().join("bin").join(bin_exe))
        .filter(|p| p.is_file())
        .collect();
    matches.sort();
    matches.pop()
}

#[cfg(target_os = "macos")]
fn detect_one(app: OpenInApp, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let app_dirs: Vec<PathBuf> = [
        Some(PathBuf::from("/Applications")),
        home.as_ref().map(|h| h.join("Applications")),
    ]
    .into_iter()
    .flatten()
    .collect();

    let find_bundle = |prefix: &str, inner_cli: Option<&str>| -> Option<PathBuf> {
        for dir in &app_dirs {
            let exact = dir.join(format!("{prefix}.app"));
            let bundle = if exact.is_dir() {
                Some(exact)
            } else {
                let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_dir()
                            && p.file_name()
                                .and_then(OsStr::to_str)
                                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".app"))
                    })
                    .collect();
                found.sort();
                found.pop()
            };
            if let Some(bundle) = bundle {
                if let Some(rel) = inner_cli {
                    let cli = bundle.join(rel);
                    if cli.is_file() {
                        return Some(cli);
                    }
                }
                return Some(bundle);
            }
        }
        None
    };

    match app {
        OpenInApp::VsCode => find_on_path(path_dirs, &["code"]).or_else(|| {
            find_bundle(
                "Visual Studio Code",
                Some("Contents/Resources/app/bin/code"),
            )
        }),
        OpenInApp::VsCodeInsiders => find_on_path(path_dirs, &["code-insiders"]).or_else(|| {
            find_bundle(
                "Visual Studio Code - Insiders",
                Some("Contents/Resources/app/bin/code"),
            )
        }),
        OpenInApp::Cursor => find_on_path(path_dirs, &["cursor"])
            .or_else(|| find_bundle("Cursor", Some("Contents/Resources/app/bin/cursor"))),
        OpenInApp::Windsurf => find_on_path(path_dirs, &["windsurf"])
            .or_else(|| find_bundle("Windsurf", Some("Contents/Resources/app/bin/windsurf"))),
        OpenInApp::Zed => find_on_path(path_dirs, &["zed"])
            .or_else(|| find_bundle("Zed", Some("Contents/MacOS/cli"))),
        OpenInApp::Qoder => {
            find_on_path(path_dirs, &["qoder"]).or_else(|| find_bundle("Qoder", None))
        }
        OpenInApp::SublimeText => find_on_path(path_dirs, &["subl"])
            .or_else(|| find_bundle("Sublime Text", Some("Contents/SharedSupport/bin/subl"))),
        OpenInApp::IntelliJ => {
            find_on_path(path_dirs, &["idea"]).or_else(|| find_bundle("IntelliJ IDEA", None))
        }
        OpenInApp::CLion => {
            find_on_path(path_dirs, &["clion"]).or_else(|| find_bundle("CLion", None))
        }
        OpenInApp::RustRover => {
            find_on_path(path_dirs, &["rustrover"]).or_else(|| find_bundle("RustRover", None))
        }
        OpenInApp::PyCharm => {
            find_on_path(path_dirs, &["pycharm"]).or_else(|| find_bundle("PyCharm", None))
        }
        OpenInApp::WebStorm => {
            find_on_path(path_dirs, &["webstorm"]).or_else(|| find_bundle("WebStorm", None))
        }
        OpenInApp::GoLand => {
            find_on_path(path_dirs, &["goland"]).or_else(|| find_bundle("GoLand", None))
        }
        OpenInApp::FileManager => None,
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn detect_one(app: OpenInApp, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    let names: &[&str] = match app {
        OpenInApp::VsCode => &["code"],
        OpenInApp::VsCodeInsiders => &["code-insiders"],
        OpenInApp::Cursor => &["cursor"],
        OpenInApp::Windsurf => &["windsurf"],
        OpenInApp::Zed => &["zed", "zeditor"],
        OpenInApp::Qoder => &["qoder"],
        OpenInApp::SublimeText => &["subl"],
        OpenInApp::IntelliJ => &["idea", "idea.sh"],
        OpenInApp::CLion => &["clion", "clion.sh"],
        OpenInApp::RustRover => &["rustrover", "rustrover.sh"],
        OpenInApp::PyCharm => &["pycharm", "pycharm.sh"],
        OpenInApp::WebStorm => &["webstorm", "webstorm.sh"],
        OpenInApp::GoLand => &["goland", "goland.sh"],
        OpenInApp::FileManager => return None,
    };
    find_on_path(path_dirs, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(app: OpenInApp, exe: &str) -> DetectedApp {
        DetectedApp {
            app,
            executable: PathBuf::from(exe),
        }
    }

    #[test]
    fn every_app_id_round_trips() {
        for &app in OpenInApp::ALL_EDITORS
            .iter()
            .chain(std::iter::once(&OpenInApp::FileManager))
        {
            assert_eq!(OpenInApp::from_id(app.id()), Some(app));
        }
        assert_eq!(OpenInApp::from_id("unknown-editor"), None);
    }

    #[test]
    fn format_and_normalize_ssh_authority_preserves_user_port_and_ipv6() {
        assert_eq!(
            format_ssh_authority("", "devbox", 22).as_deref(),
            Some("devbox")
        );
        assert_eq!(
            format_ssh_authority("alice", "devbox", 22).as_deref(),
            Some("alice@devbox")
        );
        assert_eq!(
            format_ssh_authority("alice", "devbox", 2222).as_deref(),
            Some("alice@devbox:2222")
        );
        assert_eq!(
            format_ssh_authority("", "devbox", 2222).as_deref(),
            Some("devbox:2222")
        );
        assert_eq!(
            format_ssh_authority("", "::1", 22).as_deref(),
            Some("[::1]")
        );
        assert_eq!(
            format_ssh_authority("alice", "2001:db8::1", 2222).as_deref(),
            Some("alice@[2001:db8::1]:2222")
        );
        assert_eq!(
            format_ssh_authority("", "[::1]", 2222).as_deref(),
            Some("[::1]:2222")
        );
        assert_eq!(format_ssh_authority("alice", "  ", 22), None);

        assert_eq!(
            normalize_ssh_authority(" alice@devbox:2222 ").as_deref(),
            Some("alice@devbox:2222")
        );
        assert_eq!(
            normalize_ssh_authority("alice@2001:db8::1").as_deref(),
            Some("alice@[2001:db8::1]")
        );
        assert_eq!(normalize_ssh_authority("::1").as_deref(), Some("[::1]"));
        assert_eq!(
            normalize_ssh_authority("[::1]:2222").as_deref(),
            Some("[::1]:2222")
        );
        assert_eq!(normalize_ssh_authority("   "), None);
    }

    #[test]
    fn resolve_primary_respects_preference_and_location_capabilities() {
        let installed = vec![
            fake(OpenInApp::IntelliJ, "idea64.exe"),
            fake(OpenInApp::Zed, "zed.exe"),
            fake(OpenInApp::VsCode, "Code.exe"),
            fake(OpenInApp::Cursor, "Cursor.exe"),
        ];
        let local = OpenLocation::Local(PathBuf::from(r"D:\gh-prj\hitty"));
        let ssh = OpenLocation::RemoteSsh {
            authority: "alice@devbox:2222".into(),
            path: "/home/alice/hitty".into(),
        };
        let wsl = OpenLocation::Wsl {
            distro: "Ubuntu".into(),
            path: "/home/alice/hitty".into(),
        };
        let unsupported = OpenLocation::RemoteUnsupported {
            path: "/srv/hitty".into(),
        };

        assert_eq!(
            resolve_primary(&installed, Some(OpenInApp::Cursor), &local),
            Some(OpenInApp::Cursor)
        );
        assert_eq!(
            resolve_primary(&installed, Some(OpenInApp::FileManager), &local),
            Some(OpenInApp::FileManager)
        );
        // Saved local-only app falls back to the first SSH-capable editor over SSH.
        assert_eq!(
            resolve_primary(&installed, Some(OpenInApp::IntelliJ), &ssh),
            Some(OpenInApp::Zed)
        );
        assert_eq!(
            resolve_primary(&installed, Some(OpenInApp::FileManager), &ssh),
            Some(OpenInApp::Zed)
        );
        // Over WSL, Zed is skipped and VS Code is chosen.
        assert_eq!(
            resolve_primary(&installed, Some(OpenInApp::Zed), &wsl),
            Some(OpenInApp::VsCode)
        );
        // Unsupported remote locations never pick any external editor.
        assert_eq!(
            resolve_primary(&installed, Some(OpenInApp::VsCode), &unsupported),
            None
        );
        // Empty installed list falls back to FileManager locally and None remotely.
        assert_eq!(
            resolve_primary(&[], None, &local),
            Some(OpenInApp::FileManager)
        );
        assert_eq!(resolve_primary(&[], None, &ssh), None);
    }

    #[test]
    fn macos_app_bundle_capabilities_and_errors_stay_consistent() {
        let vscode_app = fake(OpenInApp::VsCode, "/Applications/Visual Studio Code.app");
        let zed_app = fake(OpenInApp::Zed, "/Applications/Zed.app");
        let local = OpenLocation::Local(PathBuf::from("/Users/dev/hitty"));
        let ssh = OpenLocation::RemoteSsh {
            authority: "alice@devbox:2222".into(),
            path: "/srv/hitty".into(),
        };
        let wsl = OpenLocation::Wsl {
            distro: "Ubuntu".into(),
            path: "/srv/hitty".into(),
        };

        // `.app` bundle works locally, but needs its inner CLI for Remote-SSH/WSL.
        assert!(vscode_app.supports_location(&local));
        assert!(!vscode_app.supports_location(&ssh));
        assert_eq!(
            vscode_app.location_error(&ssh),
            Some(OpenInError::MissingMacosCli(OpenInApp::VsCode))
        );
        assert!(!vscode_app.supports_location(&wsl));
        assert_eq!(
            vscode_app.location_error(&wsl),
            Some(OpenInError::MissingMacosCli(OpenInApp::VsCode))
        );

        // `Zed.app` opens `ssh://` URIs via `open -a`, so it works over SSH,
        // and rejects WSL as UnsupportedLocation (not LocalOnly).
        assert!(zed_app.supports_location(&ssh));
        assert!(!zed_app.supports_location(&wsl));
        assert_eq!(
            zed_app.location_error(&wsl),
            Some(OpenInError::UnsupportedLocation(OpenInApp::Zed))
        );

        let bundle_only = vec![vscode_app, zed_app];
        assert_eq!(
            resolve_primary(&bundle_only, Some(OpenInApp::VsCode), &ssh),
            Some(OpenInApp::Zed)
        );
        assert_eq!(
            resolve_primary(&bundle_only, Some(OpenInApp::VsCode), &wsl),
            None
        );
    }

    #[test]
    fn launch_command_handles_local_ssh_wsl_and_rejects_invalid_remote() {
        let vscode = fake(
            OpenInApp::VsCode,
            r"C:\Users\dev\AppData\Local\Programs\Microsoft VS Code\Code.exe",
        );
        let local = build_launch_command(
            &vscode,
            &OpenLocation::Local(PathBuf::from(r"\\?\D:\My Projects\100% done")),
        )
        .unwrap();
        assert_eq!(local.program, vscode.executable);
        assert_eq!(local.args, vec![r"D:\My Projects\100% done"]);

        let ssh = build_launch_command(
            &vscode,
            &OpenLocation::RemoteSsh {
                authority: "alice@devbox:2222".into(),
                path: "/home/dev/hitty".into(),
            },
        )
        .unwrap();
        assert_eq!(
            ssh.args,
            vec![
                "--remote",
                "ssh-remote+alice@devbox:2222",
                "/home/dev/hitty"
            ]
        );

        let wsl = build_launch_command(
            &vscode,
            &OpenLocation::Wsl {
                distro: "Ubuntu".into(),
                path: "/home/dev/hitty".into(),
            },
        )
        .unwrap();
        assert_eq!(wsl.args, vec!["--remote", "wsl+Ubuntu", "/home/dev/hitty"]);

        let zed = fake(OpenInApp::Zed, "/usr/bin/zed");
        let zed_ssh = build_launch_command(
            &zed,
            &OpenLocation::RemoteSsh {
                authority: "alice@devbox:2222".into(),
                path: "/srv/app".into(),
            },
        )
        .unwrap();
        assert_eq!(zed_ssh.args, vec!["ssh://alice@devbox:2222/srv/app"]);
        assert_eq!(
            build_launch_command(&zed, &wsl_location()),
            Err(OpenInError::UnsupportedLocation(OpenInApp::Zed))
        );

        // Empty SSH authority and RemoteUnsupported must be rejected, never emitting invalid args.
        assert_eq!(
            build_launch_command(
                &vscode,
                &OpenLocation::RemoteSsh {
                    authority: "  ".into(),
                    path: "/srv/app".into(),
                },
            ),
            Err(OpenInError::UnsupportedLocation(OpenInApp::VsCode))
        );
        assert_eq!(
            build_launch_command(
                &vscode,
                &OpenLocation::RemoteUnsupported {
                    path: "/srv/app".into(),
                },
            ),
            Err(OpenInError::UnsupportedLocation(OpenInApp::VsCode))
        );

        let idea = fake(OpenInApp::IntelliJ, "idea64.exe");
        assert_eq!(
            build_launch_command(
                &idea,
                &OpenLocation::RemoteSsh {
                    authority: "devbox".into(),
                    path: "/srv/app".into(),
                },
            ),
            Err(OpenInError::LocalOnly(OpenInApp::IntelliJ))
        );
        assert_eq!(
            build_launch_command(&idea, &wsl_location()),
            Err(OpenInError::LocalOnly(OpenInApp::IntelliJ))
        );
    }

    fn wsl_location() -> OpenLocation {
        OpenLocation::Wsl {
            distro: "Ubuntu".into(),
            path: "/srv/app".into(),
        }
    }

    #[test]
    fn windows_editor_detection_resolves_native_executable_from_batch_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("resources").join("app").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("cursor.cmd"), []).unwrap();
        let roots = WindowsSearchRoots {
            path_dirs: &[bin],
            toolbox_scripts: None,
            program_files: None,
            local_programs: None,
        };
        assert_eq!(
            find_windows_editor(&roots, "Cursor", "Cursor.exe", "cursor.cmd"),
            None
        );

        let executable = dir.path().join("Cursor.exe");
        std::fs::write(&executable, []).unwrap();
        assert_eq!(
            find_windows_editor(&roots, "Cursor", "Cursor.exe", "cursor.cmd"),
            Some(executable)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_editor_launch_requires_a_native_executable() {
        let launcher = fake(OpenInApp::VsCode, "code.cmd");
        assert!(matches!(
            launch_editor(&launcher, &OpenLocation::Local(PathBuf::from(r"D:\work"))),
            Err(OpenInError::SpawnFailed { program, reason })
                if program == launcher.executable && reason == "expected a native Windows editor executable"
        ));
    }

    #[test]
    fn jetbrains_detection_ignores_empty_dirs_and_picks_newest_version() {
        let dir = tempfile::tempdir().unwrap();
        let jb = dir.path().join("JetBrains");
        std::fs::create_dir_all(jb.join("IntelliJ IDEA 2022.1").join("bin")).unwrap();
        std::fs::write(
            jb.join("IntelliJ IDEA 2022.1")
                .join("bin")
                .join("idea64.exe"),
            b"",
        )
        .unwrap();
        std::fs::create_dir_all(jb.join("IntelliJ IDEA 2024.3").join("bin")).unwrap();
        std::fs::write(
            jb.join("IntelliJ IDEA 2024.3")
                .join("bin")
                .join("idea64.exe"),
            b"",
        )
        .unwrap();
        // Empty uninstalled directory must not win.
        std::fs::create_dir_all(jb.join("IntelliJ IDEA 2026.1")).unwrap();

        let roots = WindowsSearchRoots {
            path_dirs: &[],
            toolbox_scripts: None,
            program_files: Some(dir.path()),
            local_programs: None,
        };
        let found = detect_jetbrains_windows(&roots, "IntelliJ IDEA", "idea64.exe");
        assert_eq!(
            found,
            Some(
                jb.join("IntelliJ IDEA 2024.3")
                    .join("bin")
                    .join("idea64.exe")
            )
        );
    }
}
