#[cfg(any(windows, target_os = "macos"))]
use raw_window_handle::HasWindowHandle;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, Focusable as _, PromptLevel, SharedString, Subscription, Window,
    div, px, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState, Position, TabSize};
use gpui_component::menu::ContextMenuExt as _;
use gpui_component::notification::Notification;
use gpui_component::scroll::ScrollbarShow;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};

use crate::core::config::Config;
use crate::ui::app::Tty7App;
use crate::ui::document_column::DocumentChrome;
use crate::ui::host_ops::{HostId, HostOps, MTime, SharedHost, WatchSub};
use crate::ui::i18n::{L10nKey, t, t_fmt};

const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// File-tab labels follow the UI scale: 13px at the default 16px rem.
const FILE_TAB_TEXT: f32 = 13. / 16.;

/// Windows Terminal geometry for the file-tab rail, in fixed px because the
/// rail height is fixed: the strip keeps a band above the tabs and a lead
/// before the first one, the selected tab is a rounded "sheet" in the
/// document color that meets the content below, and separators are short
/// hairlines inset from the sheet's edges.
const FILE_TAB_INSET: f32 = 6.;
const FILE_TAB_LEAD: f32 = 7.;
const FILE_TAB_RADIUS: f32 = 5.;
const FILE_TAB_SEP_INSET: f32 = 6.;

#[cfg(all(feature = "markdown-visual-tests", any(windows, target_os = "macos")))]
pub(crate) mod native_html;

const RELOAD_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(200);

#[cfg(any(windows, target_os = "macos"))]
async fn html_evaluation(receiver: Option<smol::channel::Receiver<String>>) -> Option<String> {
    use smol::future::FutureExt as _;
    let receiver = receiver?;
    async { receiver.recv().await.ok() }
        .or(async {
            // A hung renderer is not guaranteed to complete script evaluation.
            smol::Timer::after(std::time::Duration::from_secs(3)).await;
            None
        })
        .await
}

pub(crate) struct OpenFile {
    pub(crate) path: PathBuf,
    /// The machine `path` lives on, held rather than looked up. Saves,
    /// reloads and duplicate detection all key on its id — an SFTP file and a
    /// local file can share the string `/etc/hosts` without being the same
    /// file — and saving goes straight through this handle, so a buffer stays
    /// saveable however the window's own machine has changed underneath it.
    pub(crate) host: SharedHost,
    pub(crate) input: Entity<InputState>,
    pub(crate) dirty: bool,
    disk_mtime: Option<MTime>,
    edit_seq: u64,
    saving: Option<u64>,
    save_pending: bool,
    save_then_close: bool,
    reload_seq: u64,
    pub(crate) conflict: bool,
    pub(crate) preview: bool,
    pub(crate) wrap: bool,
    pub(crate) reading: Option<Entity<crate::ui::markdown_preview::MarkdownPreview>>,
    #[cfg(any(windows, target_os = "macos"))]
    html_scroll: Option<[f64; 2]>,
    source_highlighter_ready: std::cell::Cell<bool>,
    _sub: Subscription,
    _observe: Subscription,
}

impl OpenFile {
    fn prepare_source(&self, cx: &mut gpui::App) {
        if !self.source_highlighter_ready.replace(true) {
            self.input.update(cx, |input, cx| {
                input.set_highlighter(crate::ui::editor_theme::language("markdown"), cx);
            });
        }
    }

    fn label(&self) -> SharedString {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.display().to_string())
            .into()
    }
}

pub(crate) struct TabCode {
    pub(crate) visible: bool,
    pub(crate) files: Vec<OpenFile>,
    pub(crate) active: usize,
    tab_scroll: gpui::ScrollHandle,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) expanded: std::collections::HashSet<PathBuf>,
    /// Roots start open. Their folds belong to this tab and last for the session.
    pub(crate) collapsed_roots: HashSet<PathBuf>,
    pub(crate) selected: Option<crate::ui::file_tree::TreeSelection>,
    pub(crate) selection: Vec<crate::ui::file_tree::TreeSelection>,
    pub(crate) selection_anchor: Option<crate::ui::file_tree::TreeSelection>,
    pub(crate) selection_host: Option<HostId>,
    pub(crate) selection_query: String,
}

impl TabCode {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            files: Vec::new(),
            active: 0,
            tab_scroll: gpui::ScrollHandle::new(),
            roots: Vec::new(),
            expanded: std::collections::HashSet::new(),
            collapsed_roots: HashSet::new(),
            selected: None,
            selection: Vec::new(),
            selection_anchor: None,
            selection_host: None,
            selection_query: String::new(),
        }
    }

    pub(crate) fn select_tree(&mut self, selection: crate::ui::file_tree::TreeSelection) {
        self.selected = Some(selection.clone());
        self.selection_anchor = Some(selection.clone());
        self.selection = vec![selection];
    }

    pub(crate) fn active_file(&self) -> Option<&OpenFile> {
        self.files.get(self.active)
    }

    pub(crate) fn active_display_path(&self, tree_host: HostId) -> Option<PathBuf> {
        let file = self.active_file()?;
        let path = crate::ui::path_display::readable_path(&file.path, file.host.id());
        // Start at the matching project name, without its absolute ancestors.
        // Roots on another host cannot label this buffer.
        if file.host.id() == tree_host
            && let Some((_, relative)) = self
                .roots
                .iter()
                .filter_map(|root| {
                    let root = crate::ui::path_display::readable_path(root, tree_host);
                    let relative = path.strip_prefix(&root).ok()?;
                    let project = root.file_name().map(PathBuf::from).unwrap_or_default();
                    Some((root.components().count(), project.join(relative)))
                })
                .max_by_key(|(depth, _)| *depth)
        {
            return Some(relative);
        }
        // The tree can change hosts or refresh its roots while a file stays
        // open. Keep absolute ancestors in the full-path tooltip only.
        path.file_name().map(PathBuf::from)
    }
}

enum EditorNavigation {
    Source { line: u32, column: u32 },
    PreviewAnchor(String),
}

pub(crate) struct EditorPanelState {
    #[cfg(any(windows, target_os = "macos"))]
    pub(crate) html: Option<Entity<super::html_preview::HtmlPreview>>,
    #[cfg(any(windows, target_os = "macos"))]
    html_loading: bool,
    #[cfg(any(windows, target_os = "macos"))]
    html_busy: bool,
    /// Only the latest open request may install a file or move focus. Its
    /// navigation target travels with the request and is dropped on failure.
    open_seq: u64,
    watch: Option<Arc<WatchSub>>,
    watch_host: Option<SharedHost>,
    watch_opening: bool,
    watch_busy: bool,
    watch_dirty: bool,
    watched_dirs: HashSet<PathBuf>,
    watched_files: HashSet<PathBuf>,
    events_tx: smol::channel::Sender<Vec<PathBuf>>,
}

impl EditorPanelState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Tty7App>) -> Self {
        let (tx, rx) = smol::channel::unbounded::<Vec<PathBuf>>();
        cx.spawn_in(window, async move |app, cx| {
            while let Ok(first) = rx.recv().await {
                cx.background_executor().timer(RELOAD_DEBOUNCE).await;
                let mut changed: HashSet<PathBuf> = first.into_iter().collect();
                while let Ok(more) = rx.try_recv() {
                    changed.extend(more);
                }
                let ok = app.update_in(cx, |app, window, cx| {
                    for path in changed {
                        if app.editor.watched_files.contains(&path) {
                            app.editor_handle_external_change(&path, window, cx);
                        }
                    }
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            #[cfg(any(windows, target_os = "macos"))]
            html: None,
            #[cfg(any(windows, target_os = "macos"))]
            html_loading: false,
            #[cfg(any(windows, target_os = "macos"))]
            html_busy: false,
            open_seq: 0,
            watch: None,
            watch_host: None,
            watch_opening: false,
            watch_busy: false,
            watch_dirty: false,
            watched_dirs: HashSet::new(),
            watched_files: HashSet::new(),
            events_tx: tx,
        }
    }
}

pub(crate) fn language_for_path(path: &Path) -> &'static str {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lowered = name.to_ascii_lowercase();
        match lowered.as_str() {
            "makefile" | "gnumakefile" => return "make",
            "cmakelists.txt" => return "cmake",
            _ => {}
        }
        if lowered.starts_with('.') && (lowered.contains("shrc") || lowered.ends_with("profile")) {
            return "bash";
        }
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return "text";
    };
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "go" => "go",
        "py" | "pyi" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "json" | "jsonc" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "html" | "htm" => "html",
        "css" => "css",
        "md" | "markdown" => "markdown",
        "sh" | "bash" | "zsh" => "bash",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "lua" => "lua",
        "rb" => "ruby",
        "php" => "php",
        "sql" => "sql",
        "swift" => "swift",
        "scala" => "scala",
        "zig" => "zig",
        "proto" => "proto",
        "diff" | "patch" => "diff",
        "ex" | "exs" => "elixir",
        "erb" => "erb",
        "ejs" => "ejs",
        "svelte" => "svelte",
        "astro" => "astro",
        "graphql" | "gql" => "graphql",
        "cs" => "csharp",
        "cmake" => "cmake",
        _ => "text",
    }
}

fn can_preview_html(file: &OpenFile) -> bool {
    cfg!(any(windows, target_os = "macos"))
        && file.host.id().is_local()
        && language_for_path(&file.path) == "html"
}

/// How many frames a jump-to-line may wait for the editor to be laid out.
///
/// `InputState::scroll_to` gives up when the buffer has never been painted,
/// and that is exactly the state a file that just opened is in: the cursor
/// lands on the right line and the view stays at the top of the file, which
/// is the one thing a `:120` link exists to avoid. Three frames is the same
/// bounded-retry shape `prefill::select_all_when_drawn` uses against the same
/// class of problem.
const CURSOR_SCROLL_ATTEMPTS: u8 = 3;

/// Puts the cursor at `position`, re-trying on later frames until the scroll
/// that follows it can actually be computed.
///
/// Applied immediately as well as on the retry: the cursor itself lands with
/// no layout, so the position is right even for a pane that never paints.
fn place_cursor(
    input: Entity<InputState>,
    position: Position,
    left: u8,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    input.update(cx, |state, cx| {
        state.set_cursor_position(position, window, cx);
    });
    // A target near the top of the file scrolls nowhere and is already done;
    // so is one that has landed. Either way this stops.
    if left <= 1 || input.read(cx).scroll_offset().y != px(0.) {
        return;
    }
    window.on_next_frame(move |window, cx| {
        place_cursor(input, position, left - 1, window, cx);
    });
    // Registering a callback does not by itself ask for a frame, and an
    // overlay that has finished drawing has no other reason to produce one.
    window.refresh();
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

fn is_archive(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab"
            )
        })
}

enum EditorOpenError {
    Binary(String),
    Message(String),
}

impl From<String> for EditorOpenError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ExternalChange {
    Ignore,
    Conflict,
    Reload,
}

fn classify_external_change(
    saving: bool,
    dirty: bool,
    disk_mtime: Option<MTime>,
    observed: Option<MTime>,
) -> ExternalChange {
    if saving {
        return ExternalChange::Ignore;
    }
    if observed.is_some() && observed == disk_mtime {
        return ExternalChange::Ignore;
    }
    if dirty {
        ExternalChange::Conflict
    } else {
        ExternalChange::Reload
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SaveLanding {
    clean: bool,
    requeue: bool,
}

fn settle_save(ok: bool, wrote_seq: u64, current_seq: u64, pending: bool) -> SaveLanding {
    SaveLanding {
        clean: ok && wrote_seq == current_seq,
        requeue: ok && pending,
    }
}

impl Tty7App {
    pub(crate) fn reveal_commit_document_tab(&self) {
        if let Some(code) = self.tab_code() {
            code.tab_scroll.scroll_to_item(code.files.len());
        }
    }
    pub(crate) fn tab_code(&self) -> Option<&TabCode> {
        self.tabs.get(self.active)?.code.as_deref()
    }

    pub(crate) fn tab_code_mut(&mut self) -> Option<&mut TabCode> {
        self.tabs.get_mut(self.active)?.code.as_deref_mut()
    }

    pub(crate) fn tab_code_mut_or_init(&mut self) -> Option<&mut TabCode> {
        let tab = self.tabs.get_mut(self.active)?;
        Some(tab.code.get_or_insert_with(|| Box::new(TabCode::new())))
    }

    pub(crate) fn code_panel_visible(&self) -> bool {
        self.tab_code().is_some_and(|c| c.visible)
    }

    fn editor_rebuild_watcher(&mut self, cx: &mut Context<Self>) {
        // Only files on the host the watch itself runs on. A path from
        // another machine — an SFTP file, say — does not exist under that
        // watcher's feet, and would either miss or, worse, match a local file
        // that happens to share its name.
        //
        // A host that cannot watch therefore gets no external-change
        // detection at all: an SFTP buffer will not notice the file changing
        // underneath it, and saving overwrites whatever is there. Catching
        // that at save time needs a "keep mine" that survives to the next
        // save, which the conflict banner does not have yet.
        let watch_host = self.spawn_host(cx);
        let files: HashSet<PathBuf> = self
            .tabs
            .iter()
            .filter_map(|t| t.code.as_deref())
            .flat_map(|c| c.files.iter())
            .filter(|f| f.host.id() == watch_host)
            .map(|f| f.path.clone())
            .collect();
        let dirs: HashSet<PathBuf> = files
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        self.editor.watched_files = files;
        if dirs == self.editor.watched_dirs {
            return;
        }
        self.editor.watched_dirs = dirs;
        self.editor_watch_apply(cx);
    }

    fn editor_watch_apply(&mut self, cx: &mut Context<Self>) {
        let want: Vec<PathBuf> = self.editor.watched_dirs.iter().cloned().collect();
        let Some(host) = self.active_host(cx) else {
            return;
        };

        if !self
            .editor
            .watch_host
            .as_ref()
            .is_some_and(|opened_with| Arc::ptr_eq(opened_with, &host))
        {
            self.editor.watch = None;
            self.editor.watch_host = None;
            self.editor.watch_busy = false;
            self.editor.watch_dirty = false;
        }

        if let Some(sub) = self.editor.watch.clone() {
            if self.editor.watch_busy {
                self.editor.watch_dirty = true;
                return;
            }
            self.editor.watch_busy = true;
            HostOps::run(
                host,
                cx,
                move |_| sub.set_dirs(&want),
                |app: &mut Self, result: std::io::Result<()>, cx| {
                    app.editor.watch_busy = false;
                    if let Err(e) = result {
                        log::warn!("editor: could not update the watched set: {e}");
                    }
                    if std::mem::take(&mut app.editor.watch_dirty) {
                        app.editor_watch_apply(cx);
                    }
                },
            );
            return;
        }
        if self.editor.watch_opening {
            return;
        }
        self.editor.watch_opening = true;
        let opened_host = Arc::clone(&host);
        let opened_with = self.editor.watched_dirs.clone();
        HostOps::run(
            host,
            cx,
            {
                let want = want.clone();
                move |h| h.watch(&want).map(Arc::new)
            },
            move |app, result: std::io::Result<Arc<WatchSub>>, cx| {
                app.editor.watch_opening = false;
                let sub = match result {
                    Ok(sub) => sub,
                    Err(e) => {
                        log::warn!("editor: external-change watcher unavailable: {e}");
                        return;
                    }
                };
                let events = sub.events().clone();
                app.editor.watch = Some(sub);
                app.editor.watch_host = Some(opened_host);
                cx.spawn(async move |app, cx| {
                    while let Ok(batch) = events.recv().await {
                        let ok = app.update(cx, |app, _cx| {
                            let _ = app.editor.events_tx.try_send(batch);
                        });
                        if ok.is_err() {
                            break;
                        }
                    }
                })
                .detach();
                if app.editor.watched_dirs != opened_with {
                    app.editor_watch_apply(cx);
                }
            },
        );
    }

    /// Shows what a file link in the grid pointed at.
    ///
    /// A file opens in the editor, on the line the link named; the tree
    /// follows along so "what does it say?" and "where does it live?" are
    /// answered by the same click. A directory has no contents to show, so it
    /// is only ever the tree.
    pub(crate) fn open_linked_file(
        &mut self,
        path: &Path,
        line: Option<u32>,
        column: Option<u32>,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_dir {
            self.file_tree_show(path, window, cx);
            return;
        }
        self.open_file_in_editor_at(path, line, column, window, cx);
        self.file_tree_reveal_path(path, cx);
    }

    /// [`Self::open_file_in_editor`], landing the cursor on a line the caller
    /// already knows — what a `src/main.rs:120:3` in the grid was pointing at.
    pub(crate) fn open_file_in_editor_at(
        &mut self,
        path: &Path,
        line: Option<u32>,
        column: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.active_host(cx) else {
            return;
        };
        self.editor_open_with_navigation(
            host,
            path,
            line.or(column.map(|_| 1))
                .map(|line| EditorNavigation::Source {
                    line,
                    column: column.unwrap_or(1),
                }),
            window,
            cx,
        );
    }

    fn apply_editor_navigation(
        &mut self,
        host: HostId,
        opened: &Path,
        navigation: Option<EditorNavigation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(navigation) = navigation else {
            return;
        };
        let Some(file) = self.tab_code_mut().and_then(|c| {
            c.files
                .iter_mut()
                .find(|f| f.host.id() == host && f.path == *opened)
        }) else {
            return;
        };
        match navigation {
            EditorNavigation::Source { line, column } => {
                file.prepare_source(cx);
                let input = file.input.clone();
                file.preview = false;
                input.update(cx, |input, cx| input.focus(window, cx));
                // File links count from one; Position counts from zero.
                let position = Position {
                    line: line.saturating_sub(1),
                    character: column.saturating_sub(1),
                };
                place_cursor(input, position, CURSOR_SCROLL_ATTEMPTS, window, cx);
            }
            EditorNavigation::PreviewAnchor(anchor) => {
                if let Some(preview) = &file.reading {
                    file.preview = true;
                    preview.update(cx, |preview, cx| preview.navigate_anchor(anchor, cx));
                    window.focus(&preview.focus_handle(cx), cx);
                }
            }
        }
    }

    pub(crate) fn editor_open_markdown_link(
        &mut self,
        host: SharedHost,
        path: &Path,
        fragment: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_open_with_navigation(
            host,
            path,
            fragment.map(EditorNavigation::PreviewAnchor),
            window,
            cx,
        );
    }

    pub(crate) fn open_file_in_editor(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.active_host(cx) else {
            return;
        };
        self.editor_open_on_host(host, path, window, cx);
    }

    /// [`Self::open_file_in_editor`] against an explicit host — the SFTP
    /// browser's files live on a host that is never the active one.
    pub(crate) fn editor_open_on_host(
        &mut self,
        host: SharedHost,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_open_with_navigation(host, path, None, window, cx);
    }

    fn editor_open_with_navigation(
        &mut self,
        host: SharedHost,
        path: &Path,
        navigation: Option<EditorNavigation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.open_seq = self.editor.open_seq.wrapping_add(1);
        let open_seq = self.editor.open_seq;
        if self.tabs.get(self.active).is_none() {
            return;
        }
        if self.editor_activate_open(host.id(), path, window, cx) {
            self.raise_code_overlay();
            self.apply_editor_navigation(host.id(), path, navigation, window, cx);
            return;
        }
        let host_id = host.id();
        let p = path.to_path_buf();
        HostOps::run_in(
            host.clone(),
            window,
            cx,
            move |h| -> Result<(PathBuf, String, Option<MTime>), EditorOpenError> {
                let path = h.canonicalize(&p).unwrap_or(p);
                let display_path = crate::ui::path_display::readable_path(&path, host_id)
                    .display()
                    .to_string();
                let meta = match h.stat(&path) {
                    Ok(m) => m,
                    Err(e) => {
                        return Err(t_fmt(
                            L10nKey::EditorCantOpen,
                            &[("path", &display_path), ("e", &e.to_string())],
                        )
                        .into());
                    }
                };
                // Reject known archives before the size limit or a remote download.
                // Other extensions still use the existing content sniff below.
                if is_archive(&path) {
                    return Err(EditorOpenError::Binary(display_path));
                }
                if meta.len > MAX_FILE_BYTES {
                    return Err(t_fmt(
                        L10nKey::EditorFileTooLarge,
                        &[
                            ("path", &display_path),
                            ("size", &(meta.len / (1024 * 1024)).to_string()),
                        ],
                    )
                    .into());
                }
                let bytes = match h.read_file(&path, MAX_FILE_BYTES) {
                    Ok(b) => b,
                    Err(e) => {
                        return Err(t_fmt(
                            L10nKey::EditorCantRead,
                            &[("path", &display_path), ("e", &e.to_string())],
                        )
                        .into());
                    }
                };
                if looks_binary(&bytes) {
                    return Err(EditorOpenError::Binary(display_path));
                }
                let text = String::from_utf8(bytes)
                    .map_err(|_| t_fmt(L10nKey::EditorNotUtf8, &[("path", &display_path)]))?;
                Ok((path, text, meta.mtime))
            },
            move |app, opened, window, cx| {
                if app.editor.open_seq != open_seq {
                    return;
                }
                match opened {
                    Ok((path, text, mtime)) => {
                        app.raise_code_overlay();
                        app.editor_install_file(host, path.clone(), text, mtime, window, cx);
                        // Apply this request to the canonical path returned by
                        // its Host, including when the link named a symlink.
                        app.apply_editor_navigation(host_id, &path, navigation, window, cx);
                    }
                    Err(EditorOpenError::Binary(path)) => {
                        app.editor_binary_file_notice(&path, window, cx);
                    }
                    Err(EditorOpenError::Message(message)) => window.push_notification(message, cx),
                }
            },
        );
    }

    pub(crate) fn set_binary_file_notice(&mut self, on: bool, cx: &mut Context<Self>) {
        self.update_config(cx, |cfg| cfg.show_binary_file_notice = on);
    }

    fn editor_binary_file_notice(&self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.global::<Config>().show_binary_file_notice {
            return;
        }
        let app = cx.weak_entity();
        let opt_out = cx.new(|_| false);
        window.push_notification(
            Notification::info(t_fmt(L10nKey::EditorBinaryFile, &[("path", path)]))
                .title(t(L10nKey::EditorPreviewUnsupported))
                .id1::<Self>("binary-preview")
                .autohide(false)
                .content(move |_, _, cx| {
                    let app = app.clone();
                    let toggle_opt_out = opt_out.clone();
                    let confirm_opt_out = opt_out.clone();
                    v_flex()
                        .mt_2()
                        .gap_2()
                        .child(
                            Checkbox::new("editor-hide-binary-notice")
                                .debug_selector(|| "editor-hide-binary-notice".into())
                                .small()
                                .label(t(L10nKey::EditorDontShowBinaryNotice))
                                .checked(*opt_out.read(cx))
                                .on_click(cx.listener(move |_, checked: &bool, _, cx| {
                                    toggle_opt_out.update(cx, |value, _| *value = *checked);
                                    cx.notify();
                                })),
                        )
                        .child(
                            h_flex().justify_end().child(
                                Button::new("editor-binary-notice-ok")
                                    .debug_selector(|| "editor-binary-notice-ok".into())
                                    .small()
                                    .label(t(L10nKey::EditorPreviewDismiss))
                                    .on_click(cx.listener(move |notice, _, window, cx| {
                                        if *confirm_opt_out.read(cx) {
                                            let _ = app.update(cx, |app, cx| {
                                                app.set_binary_file_notice(false, cx)
                                            });
                                        }
                                        notice.dismiss(window, cx);
                                    })),
                            ),
                        )
                        .into_any_element()
                }),
            cx,
        );
    }

    fn editor_activate_open(
        &mut self,
        host: HostId,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(code) = self.tab_code_mut() else {
            return false;
        };
        let Some(ix) = code
            .files
            .iter()
            .position(|f| f.host.id() == host && f.path == *path)
        else {
            return false;
        };
        code.visible = true;
        code.active = ix;
        code.tab_scroll.scroll_to_item(ix);
        self.focus_editor(window, cx);
        cx.notify();
        true
    }

    fn editor_install_file(
        &mut self,
        host: SharedHost,
        path: PathBuf,
        text: String,
        mtime: Option<MTime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host_id = host.id();
        if self.editor_activate_open(host_id, &path, window, cx) {
            return;
        }
        if self.tabs.get(self.active).is_none() {
            return;
        }
        let language = language_for_path(&path);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                // Markdown opens in reading mode. Defer the source grammar and
                // its embedded-language parsers until the editor is visible.
                .code_editor(if language == "markdown" {
                    "text"
                } else {
                    crate::ui::editor_theme::language(language)
                })
                .multi_line(true)
                .tab_size(TabSize {
                    tab_size: 4,
                    hard_tabs: false,
                })
                .line_number(true)
                .searchable(true)
                .replaceable(true)
                .folding(true)
                .soft_wrap(false)
                .default_value(text)
        });
        let sub = cx.subscribe_in(&input, window, {
            let input_id = input.entity_id();
            move |this: &mut Tty7App, _input, ev, _window, cx| {
                if matches!(ev, InputEvent::Change) {
                    let Some(f) = this
                        .tabs
                        .iter_mut()
                        .filter_map(|t| t.code.as_deref_mut())
                        .flat_map(|c| c.files.iter_mut())
                        .find(|f| f.input.entity_id() == input_id)
                    else {
                        return;
                    };
                    f.dirty = true;
                    f.edit_seq = f.edit_seq.wrapping_add(1);
                    cx.notify();
                }
            }
        });
        let app = cx.weak_entity();
        let reading = (language == "markdown").then(|| {
            cx.new(|cx| {
                crate::ui::markdown_preview::MarkdownPreview::new(
                    input.clone(),
                    host.clone(),
                    path.clone(),
                    app,
                    true,
                    cx,
                )
            })
        });
        let preview = reading.is_some();
        #[cfg(any(windows, target_os = "macos"))]
        let preview = preview || (language == "html" && host.id().is_local());
        let tab = self
            .tabs
            .get_mut(self.active)
            .expect("checked at function entry");
        let code = tab.code.get_or_insert_with(|| Box::new(TabCode::new()));
        let observe = cx.observe(&input, |_, _, cx| cx.notify());
        code.files.push(OpenFile {
            path,
            host,
            input,
            dirty: false,
            disk_mtime: mtime,
            edit_seq: 0,
            saving: None,
            save_pending: false,
            save_then_close: false,
            reload_seq: 0,
            conflict: false,
            preview,
            wrap: false,
            reading,
            #[cfg(any(windows, target_os = "macos"))]
            html_scroll: None,
            source_highlighter_ready: std::cell::Cell::new(language != "markdown"),
            _sub: sub,
            _observe: observe,
        });
        code.active = code.files.len() - 1;
        code.tab_scroll.scroll_to_item(code.active);
        code.visible = true;
        self.editor_rebuild_watcher(cx);
        self.focus_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_code_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let buried = tab.overlay_top == crate::ui::app::OverlayTop::Diff
            && tab.diff_overlay.is_some()
            && tab.code.as_ref().is_some_and(|c| c.visible);
        tab.overlay_top = crate::ui::app::OverlayTop::Code;
        if buried {
            self.focus_editor(window, cx);
            cx.notify();
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let code = tab.code.get_or_insert_with(|| Box::new(TabCode::new()));
        if code.visible {
            code.visible = false;
            self.file_tree.editing = None;
            self.focus_active(window, cx);
            cx.notify();
            return;
        }
        code.visible = true;
        self.file_tree_refresh_roots(window, cx);
        if self.tab_code().is_some_and(|c| c.active_file().is_some()) {
            self.focus_editor(window, cx);
        } else {
            // With no file to show, the panel says "Open a file from the file
            // tree" and hands the tree the focus — but nothing was putting the
            // tree on screen, so ⌘⇧E on a fresh tab opened an empty editor
            // pointing at a panel the reader could not see or reach from
            // there. Reveal it, then focus it.
            if !self.file_tree_on_screen(cx) {
                self.set_right_panel_tab(crate::core::config::RightPanelTab::Files, cx);
            }
            self.file_tree.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    fn raise_code_overlay(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.overlay_top = crate::ui::app::OverlayTop::Code;
        }
    }

    pub(crate) fn focus_editor(&self, window: &mut Window, cx: &mut gpui::App) {
        if let Some(code) = self.tab_code() {
            code.tab_scroll.scroll_to_item(code.active);
        }
        if let Some(f) = self.tab_code().and_then(|c| c.active_file()) {
            #[cfg(any(windows, target_os = "macos"))]
            if f.preview && can_preview_html(f) {
                if let Some(html) = &self.editor.html {
                    html.update(cx, |html, cx| html.focus(window, cx));
                }
                return;
            }
            if f.preview
                && let Some(reading) = &f.reading
            {
                reading.read(cx).focus_handle(cx).focus(window, cx);
            } else {
                f.prepare_source(cx);
                f.input.update(cx, |input, cx| input.focus(window, cx));
            }
        }
    }

    pub(crate) fn editor_toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(file) = self
            .tab_code()
            .and_then(|code| code.active_file())
            .filter(|file| can_preview_html(file))
        {
            let preview = !file.preview;
            if preview {
                if file.dirty {
                    window.push_notification(t(L10nKey::EditorHtmlSaveFirst), cx);
                    return;
                }
            }
            if let Some(code) = self.tab_code_mut() {
                code.files[code.active].preview = preview;
            }
            self.editor_sync_html_preview(false, window, cx);
            self.focus_editor(window, cx);
            cx.notify();
            return;
        }
        if let Some(code) = self.tab_code_mut() {
            let ix = code.active;
            if let Some(file) = code.files.get_mut(ix).filter(|f| f.reading.is_some()) {
                file.preview = !file.preview;
            }
        }
        self.focus_editor(window, cx);
        cx.notify();
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn editor_begin_html_preview(
        &mut self,
        owner: gpui::EntityId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.html_loading || self.editor.html_busy {
            return;
        }
        let parent = match window.window_handle() {
            Ok(handle) => handle.as_raw(),
            Err(error) => {
                self.editor_fallback_html_source();
                self.focus_editor(window, cx);
                window.push_notification(
                    t_fmt(L10nKey::EditorHtmlFailed, &[("error", &error.to_string())]),
                    cx,
                );
                cx.notify();
                return;
            }
        };
        self.editor.html_loading = true;
        let tab = self.active;
        let open_seq = self.editor.open_seq;
        let original_focus = window.focused(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = super::html_preview::HtmlPreview::build(&path, parent);
            let _ = this.update_in(cx, |this, window, cx| {
                this.editor.html_loading = false;
                let requested = this.active == tab
                    && this.editor.open_seq == open_seq
                    && this.document_front() == Some(super::app::OverlayTop::Code)
                    && this
                        .tab_code()
                        .and_then(|code| code.active_file())
                        .is_some_and(|file| {
                            file.input.entity_id() == owner
                                && file.path == path
                                && file.preview
                                && can_preview_html(file)
                                && !file.dirty
                        });
                if requested {
                    match result {
                        Ok(pending) => {
                            let html = super::html_preview::HtmlPreview::attach(
                                pending, owner, window, cx,
                            );
                            this.editor.html = Some(cx.new(|_| html));
                            let html = this.editor.html.as_ref().unwrap();
                            let events = html.read(cx).events();
                            let html = html.downgrade();
                            cx.spawn_in(window, async move |this, cx| {
                                while let Ok(event) = events.recv().await {
                                    let _ = this.update_in(cx, |this, window, cx| {
                                        let Some(current) = html.upgrade() else {
                                            return;
                                        };
                                        match event {
                                            super::html_preview::Event::Ready(request) => {
                                                current.update(cx, |html, _| {
                                                    html.mark_loaded(request)
                                                });
                                                cx.notify();
                                            }
                                            super::html_preview::Event::Failed(failure) => {
                                                if !current.read(cx).is_current_failure(&failure) {
                                                    return;
                                                }
                                                let owner = current.read(cx).owner;
                                                current.update(cx, |html, _| html.failed = true);
                                                let owns_browser = this
                                                    .editor
                                                    .html
                                                    .as_ref()
                                                    .map_or(this.editor.html_busy, |html| {
                                                        html == &current
                                                    });
                                                if !owns_browser
                                                    || this
                                                        .editor_desired_html()
                                                        .is_none_or(|(desired, _)| desired != owner)
                                                {
                                                    return;
                                                }
                                                this.editor_fallback_html_source();
                                                this.focus_editor(window, cx);
                                                window.push_notification(
                                                    t_fmt(
                                                        L10nKey::EditorHtmlFailed,
                                                        &[("error", &failure.message)],
                                                    ),
                                                    cx,
                                                );
                                                cx.notify();
                                            }
                                        }
                                    });
                                }
                            })
                            .detach();
                            if let Some(code) = this.tab_code_mut() {
                                code.files[code.active].preview = true;
                            }
                            if window.focused(cx) == original_focus {
                                this.focus_editor(window, cx);
                            }
                        }
                        Err(error) => {
                            this.editor_fallback_html_source();
                            if window.focused(cx) == original_focus {
                                this.focus_editor(window, cx);
                            }
                            window.push_notification(
                                t_fmt(L10nKey::EditorHtmlFailed, &[("error", &error.to_string())]),
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn editor_open_html_browser(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self
            .tab_code()
            .and_then(|code| code.active_file())
            .filter(|file| can_preview_html(file))
        else {
            return;
        };
        self.open_html_in_browser(&file.path, window, cx);
    }

    pub(crate) fn open_html_in_browser(
        &self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_spawn_locally(cx) || language_for_path(path) != "html" {
            return;
        }
        // A tree row need not be the active editor, or even open in this tab.
        // Match the local host as well as the path before protecting its edits.
        let dirty = self
            .tabs
            .iter()
            .filter_map(|tab| tab.code.as_ref())
            .any(|code| {
                code.files
                    .iter()
                    .any(|file| file.host.id().is_local() && file.path == path && file.dirty)
            });
        if dirty {
            window.push_notification(t(L10nKey::EditorHtmlSaveFirst), cx);
        } else {
            // Windows' ShellExecute treats escaped file URLs as literal paths,
            // so Unicode and spaces must reach it as the original native path.
            #[cfg(windows)]
            if crate::ui::host_registry::HostRegistry::local(cx).is_absolute(path) {
                cx.open_url(&path.to_string_lossy());
            }
            #[cfg(not(windows))]
            if let Ok(url) = url::Url::from_file_path(path) {
                cx.open_url(url.as_str());
            }
        }
    }

    fn editor_refresh_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(any(windows, target_os = "macos"))]
        if !self.editor.html_busy
            && let Some(html) = &self.editor.html
        {
            if self
                .tab_code()
                .and_then(|code| code.active_file())
                .is_some_and(|file| file.dirty)
            {
                window.push_notification(t(L10nKey::EditorHtmlSaveFirst), cx);
            } else if let Err(error) = html.update(cx, |html, cx| html.reload(cx)) {
                self.editor_fallback_html_source();
                self.focus_editor(window, cx);
                window.push_notification(
                    t_fmt(L10nKey::EditorHtmlFailed, &[("error", &error.to_string())]),
                    cx,
                );
                cx.notify();
            } else if let Some(code) = self.tab_code_mut() {
                code.files[code.active].html_scroll = None;
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = (window, cx);
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn editor_release_html(&mut self) {
        self.editor.html = None;
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn editor_fallback_html_source(&mut self) {
        self.editor_release_html();
        if let Some(code) = self.tab_code_mut()
            && let Some(file) = code
                .files
                .get_mut(code.active)
                .filter(|f| can_preview_html(f))
        {
            file.preview = false;
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn editor_desired_html(&self) -> Option<(gpui::EntityId, PathBuf)> {
        (self.document_front() == Some(super::app::OverlayTop::Code))
            .then(|| self.tab_code().and_then(|code| code.active_file()))
            .flatten()
            .filter(|file| file.preview && can_preview_html(file) && !file.dirty)
            .map(|file| (file.input.entity_id(), file.path.clone()))
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub(crate) fn editor_needs_html_preview_sync(&self) -> bool {
        self.editor.html.is_some()
            || self
                .tab_code()
                .and_then(|code| code.active_file())
                .is_some_and(|file| file.preview && can_preview_html(file))
    }

    #[cfg(any(windows, target_os = "macos"))]
    pub(crate) fn editor_sync_html_preview(
        &mut self,
        unobstructed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .tab_code()
            .and_then(|code| code.active_file())
            .is_some_and(|file| file.preview && can_preview_html(file) && file.dirty)
        {
            if let Some(code) = self.tab_code_mut() {
                code.files[code.active].preview = false;
            }
            self.focus_editor(window, cx);
            window.push_notification(t(L10nKey::EditorHtmlSaveFirst), cx);
            cx.notify();
        }
        if self.editor.html_busy {
            if let Some(html) = &self.editor.html {
                html.update(cx, |html, cx| html.set_visible(false, window, cx));
            }
            return;
        }
        let desired = self.editor_desired_html();
        let Some(html) = self.editor.html.clone() else {
            if unobstructed && let Some((owner, path)) = desired {
                self.editor_begin_html_preview(owner, path, window, cx);
            }
            return;
        };
        let owner = html.read(cx).owner;
        if desired
            .as_ref()
            .is_none_or(|(desired, _)| *desired != owner)
        {
            // The transition owns the single hidden browser until its callback
            // completes. Later selections only update the desired document.
            html.update(cx, |html, cx| html.set_visible(false, window, cx));
            self.editor.html = None;
            self.editor.html_busy = true;
            let url = html.read(cx).file_url();
            let capture_required = html.read(cx).ready();
            let capture = html.read(cx).capture_scroll(cx);
            let original_focus = window.focused(cx);
            cx.spawn_in(window, async move |this, cx| {
                let snapshot = html_evaluation(capture).await;
                let responsive = !capture_required || snapshot.is_some();
                let _ = this.update_in(cx, |this, window, cx| {
                    this.editor.html_busy = false;
                    if let Some(snapshot) = snapshot
                        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&snapshot)
                        && value["url"].as_str() == Some(url.as_str())
                        && let (Some(x), Some(y)) = (value["x"].as_f64(), value["y"].as_f64())
                        && x.is_finite()
                        && y.is_finite()
                        && x >= 0.
                        && y >= 0.
                        && let Some(file) = this
                            .tabs
                            .iter_mut()
                            .filter_map(|tab| tab.code.as_mut())
                            .flat_map(|code| code.files.iter_mut())
                            .find(|file| file.input.entity_id() == owner)
                    {
                        file.html_scroll = Some([x, y]);
                    }
                    if responsive
                        && !html.read(cx).failed
                        && let Some((owner, path)) = this.editor_desired_html()
                    {
                        match html.update(cx, |html, cx| html.navigate(owner, &path, cx)) {
                            Ok(()) => {
                                this.editor.html = Some(html.clone());
                                if window.focused(cx) == original_focus {
                                    this.focus_editor(window, cx);
                                }
                            }
                            Err(error) => {
                                this.editor_fallback_html_source();
                                this.focus_editor(window, cx);
                                window.push_notification(
                                    t_fmt(
                                        L10nKey::EditorHtmlFailed,
                                        &[("error", &error.to_string())],
                                    ),
                                    cx,
                                );
                            }
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        if html.read(cx).needs_restore() {
            html.update(cx, |html, cx| html.set_visible(false, window, cx));
            let position = self
                .tab_code()
                .and_then(|code| code.active_file())
                .and_then(|file| file.html_scroll)
                .unwrap_or([0., 0.]);
            let restore = html.read(cx).restore_scroll(position, cx).ok();
            self.editor.html_busy = true;
            cx.spawn_in(window, async move |this, cx| {
                let restored = html_evaluation(restore).await.as_deref() == Some("true");
                let _ = this.update_in(cx, |this, window, cx| {
                    this.editor.html_busy = false;
                    if this
                        .editor
                        .html
                        .as_ref()
                        .is_some_and(|current| current == &html)
                    {
                        if restored {
                            html.update(cx, |html, _| html.restored());
                        } else {
                            this.editor_release_html();
                            if this
                                .editor_desired_html()
                                .is_some_and(|(desired, _)| desired == owner)
                            {
                                this.editor_fallback_html_source();
                                this.focus_editor(window, cx);
                                window.push_notification(
                                    t_fmt(
                                        L10nKey::EditorHtmlFailed,
                                        &[("error", "The browser did not restore the document")],
                                    ),
                                    cx,
                                );
                            }
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        let focused = window.focused(cx);
        let normal_focus = focused.as_ref().is_none_or(|focus| {
            *focus == html.read(cx).focus_handle(cx)
                || *focus == self.home_focus
                || self.file_tree.focus_handle.contains_focused(window, cx)
                || self
                    .file_search
                    .focus_handle(cx)
                    .contains_focused(window, cx)
                || self
                    .sidebar_search
                    .focus_handle(cx)
                    .contains_focused(window, cx)
                || self.tabs.get(self.active).is_some_and(|tab| {
                    tab.pane
                        .leaves()
                        .iter()
                        .any(|pane| pane.contains_focused(window, cx))
                })
        });
        // Popovers move GPUI focus to their own handle. Native child windows
        // otherwise sit above them, so hide the browser until focus returns.
        html.update(cx, |html, cx| {
            html.set_visible(unobstructed && normal_focus && html.ready(), window, cx)
        });
    }

    pub(crate) fn editor_has_focus(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.code_panel_visible()
            && self
                .tab_code()
                .and_then(|c| c.active_file())
                .is_some_and(|f| {
                    if f.preview {
                        #[cfg(any(windows, target_os = "macos"))]
                        if can_preview_html(f) {
                            return self.editor.html.as_ref().is_some_and(|html| {
                                html.read(cx).focus_handle(cx).contains_focused(window, cx)
                            });
                        }
                        f.reading.as_ref().is_some_and(|reading| {
                            reading
                                .read(cx)
                                .focus_handle(cx)
                                .contains_focused(window, cx)
                        })
                    } else {
                        f.input
                            .read(cx)
                            .focus_handle(cx)
                            .contains_focused(window, cx)
                    }
                })
    }

    pub(crate) fn editor_source_has_focus(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.tab_code()
            .and_then(|code| code.active_file())
            .is_some_and(|file| !file.preview)
            && self.editor_has_focus(window, cx)
    }

    pub(crate) fn editor_save_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .tab_code()
            .and_then(|c| c.active_file())
            .map(|f| f.input.entity_id())
        else {
            return;
        };
        self.editor_save_file(id, false, window, cx);
    }

    fn editor_save_file(
        &mut self,
        id: gpui::EntityId,
        then_close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The file's own host, not the active one: the buffer keeps pointing
        // at the machine it was read from, however the focus has moved since.
        let Some(host) = self.editor_file_mut(id).map(|f| f.host.clone()) else {
            return;
        };
        let Some(f) = self.editor_file_mut(id) else {
            return;
        };
        f.save_then_close |= then_close;
        if f.saving.is_some() {
            f.save_pending = true;
            return;
        }
        let seq = f.edit_seq;
        f.saving = Some(seq);
        let text = f.input.read(cx).text().to_string();
        let target = f.path.clone();
        let host_id = host.id();
        let saved_in = target.parent().map(std::path::Path::to_path_buf);
        HostOps::run_in(
            host,
            window,
            cx,
            move |h| h.write_file(&target, text.as_bytes()).map(|m| m.mtime),
            move |app, result: std::io::Result<Option<MTime>>, window, cx| {
                let Some(f) = app.editor_file_mut(id) else {
                    return;
                };
                f.saving = None;
                let landing = settle_save(
                    result.is_ok(),
                    seq,
                    f.edit_seq,
                    std::mem::take(&mut f.save_pending),
                );
                let wrote = result.is_ok();
                match result {
                    Ok(mtime) => {
                        f.disk_mtime = mtime;
                    }
                    Err(e) => {
                        // "Save failed" did not say which file, and with more
                        // than one editor tab open that is the first thing you
                        // need to know.
                        let name = f
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| f.path.display().to_string());
                        let context = t_fmt(L10nKey::EditorSaveFailed, &[("name", &name)]);
                        HostOps::notify_err(window, cx, &context, &e);
                    }
                }
                if landing.clean {
                    f.dirty = false;
                    f.conflict = false;
                }
                // A save is a working-tree edit the `.git` watch cannot see,
                // and the file tree only sees it while it happens to be showing
                // that directory.
                if wrote && let Some(dir) = &saved_in {
                    app.scm_invalidate_cwd(host_id, dir, cx);
                }
                if landing.requeue {
                    app.editor_save_file(id, false, window, cx);
                    cx.notify();
                    return;
                }
                let close = app
                    .editor_file_mut(id)
                    .is_some_and(|f| std::mem::take(&mut f.save_then_close) && !f.dirty);
                if close && let Some((tab_ix, ix)) = app.editor_file_position(id) {
                    app.editor_remove_file_in(tab_ix, ix, window, cx);
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    fn editor_file_mut(&mut self, id: gpui::EntityId) -> Option<&mut OpenFile> {
        self.tabs
            .iter_mut()
            .filter_map(|t| t.code.as_deref_mut())
            .flat_map(|c| c.files.iter_mut())
            .find(|f| f.input.entity_id() == id)
    }

    fn editor_file_position(&self, id: gpui::EntityId) -> Option<(usize, usize)> {
        self.tabs.iter().enumerate().find_map(|(tab_ix, t)| {
            let code = t.code.as_deref()?;
            let ix = code.files.iter().position(|f| f.input.entity_id() == id)?;
            Some((tab_ix, ix))
        })
    }

    pub(crate) fn editor_close_file(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(f) = self.tab_code().and_then(|c| c.files.get(ix)) else {
            return;
        };
        if !f.dirty {
            self.editor_remove_file(ix, window, cx);
            return;
        }
        let name = f.label();
        let answer = window.prompt(
            PromptLevel::Warning,
            &t_fmt(L10nKey::EditorUnsavedChanges, &[("name", &name)]),
            None,
            // Cancel sits between Save and Discard on purpose. The platform
            // renders the first button as the default and lays the rest out
            // beside it, so Discard was landing directly next to the key that
            // Return presses. Apple separates them for exactly this reason.
            // Three answers, so the shared helper does not fit: Save keeps
            // index 0 (rightmost, Return), Cancel takes Escape, and Discard
            // sits on the far left where nothing lands by reflex.
            &[
                gpui::PromptButton::ok(t(L10nKey::Save)),
                gpui::PromptButton::cancel(t(L10nKey::Cancel)),
                gpui::PromptButton::ok(t(L10nKey::EditorDiscard)),
            ],
            cx,
        );
        let id = f.input.entity_id();
        cx.spawn_in(window, async move |app, cx| {
            let Ok(choice) = answer.await else { return };
            let _ = app.update_in(cx, |app, window, cx| match choice {
                0 => app.editor_save_file(id, true, window, cx),
                2 => {
                    if let Some((tab_ix, ix)) = app.editor_file_position(id) {
                        app.editor_remove_file_in(tab_ix, ix, window, cx);
                    }
                }
                _ => {}
            });
        })
        .detach();
    }

    pub(crate) fn document_close_active_if_focused(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let header_focused = self.has_commit_document()
            && self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.diff_overlay.as_ref())
                .is_some_and(|overlay| overlay.document_header_focus.contains_focused(window, cx));
        if self.has_commit_document()
            && self.document_front() == Some(crate::ui::app::OverlayTop::Diff)
            && self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.diff_overlay.as_ref())
                .is_some_and(|overlay| {
                    overlay.focus_handle.contains_focused(window, cx) || header_focused
                })
        {
            self.close_diff_overlay(window, cx);
            return true;
        }
        if !self.editor_has_focus(window, cx) && !header_focused {
            return false;
        }
        let Some(code) = self.tab_code_mut() else {
            return false;
        };
        if code.files.is_empty() {
            code.visible = false;
            cx.notify();
            return true;
        }
        let active = code.active;
        self.editor_close_file(active, window, cx);
        true
    }

    fn editor_remove_file(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_remove_file_in(self.active, ix, window, cx);
    }

    fn editor_remove_file_in(
        &mut self,
        tab_ix: usize,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(code) = self
            .tabs
            .get_mut(tab_ix)
            .and_then(|t| t.code.as_deref_mut())
        else {
            return;
        };
        if ix >= code.files.len() {
            return;
        }
        code.files.remove(ix);
        if code.active >= ix && code.active > 0 {
            code.active -= 1;
        }
        code.tab_scroll.scroll_to_item(code.active);
        let empty = code.files.is_empty();
        if empty {
            code.visible = false;
        }
        if tab_ix == self.active {
            if empty || self.document_front() == Some(crate::ui::app::OverlayTop::Diff) {
                self.focus_active(window, cx);
            } else {
                self.focus_editor(window, cx);
            }
        }
        self.editor_rebuild_watcher(cx);
        cx.notify();
    }

    pub(crate) fn editor_handle_external_change(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.active_host(cx) else {
            return;
        };
        let host_id = host.id();
        let p = path.to_path_buf();
        let landed = p.clone();
        HostOps::run_in(
            host,
            window,
            cx,
            move |h| h.stat(&p).ok().and_then(|m| m.mtime),
            move |app, mtime, window, cx| {
                app.editor_apply_external_change(host_id, &landed, mtime, window, cx)
            },
        );
    }

    fn editor_apply_external_change(
        &mut self,
        host: HostId,
        path: &Path,
        mtime: Option<MTime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut reload: Vec<(usize, usize)> = Vec::new();
        let mut changed = false;
        for (tab_ix, tab) in self.tabs.iter_mut().enumerate() {
            let Some(code) = tab.code.as_deref_mut() else {
                continue;
            };
            for (ix, f) in code.files.iter_mut().enumerate() {
                if f.host.id() != host || f.path != *path {
                    continue;
                }
                match classify_external_change(f.saving.is_some(), f.dirty, f.disk_mtime, mtime) {
                    ExternalChange::Ignore => {}
                    ExternalChange::Conflict => {
                        f.conflict = true;
                        changed = true;
                    }
                    ExternalChange::Reload => reload.push((tab_ix, ix)),
                }
            }
        }
        for (tab_ix, ix) in reload {
            self.editor_reload_from_disk(tab_ix, ix, window, cx);
        }
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn editor_reload_from_disk(
        &mut self,
        tab_ix: usize,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(f) = self
            .tabs
            .get_mut(tab_ix)
            .and_then(|t| t.code.as_deref_mut())
            .and_then(|c| c.files.get_mut(ix))
        else {
            return;
        };
        let target = f.path.clone();
        let host = f.host.clone();
        let id = f.input.entity_id();
        f.reload_seq = f.reload_seq.wrapping_add(1);
        let seq = f.reload_seq;
        HostOps::run_in(
            host,
            window,
            cx,
            move |h| {
                let bytes = h.read_file(&target, MAX_FILE_BYTES)?;
                let text = String::from_utf8(bytes).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "not valid UTF-8")
                })?;
                let mtime = h.stat(&target).ok().and_then(|m| m.mtime);
                Ok((text, mtime))
            },
            move |app, result: std::io::Result<(String, Option<MTime>)>, window, cx| {
                let Some(f) = app.editor_file_mut(id) else {
                    return;
                };
                if f.reload_seq != seq {
                    return;
                }
                let Ok((text, mtime)) = result else {
                    // A failed reload did not edit the buffer. Preserve its
                    // dirty state so a deleted, unedited file can still close.
                    f.conflict = false;
                    cx.notify();
                    return;
                };
                f.disk_mtime = mtime;
                f.dirty = false;
                f.conflict = false;
                f.edit_seq = f.edit_seq.wrapping_add(1);
                let input = f.input.clone();
                input.update(cx, |input, cx| input.set_value(text, window, cx));
                cx.notify();
            },
        );
    }
}

impl Tty7App {
    pub(crate) fn render_code_overlay(
        &mut self,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.has_commit_document()
            && self.document_front() == Some(crate::ui::app::OverlayTop::Diff)
        {
            return None;
        }
        if !self.code_panel_visible() {
            return None;
        }
        let body = match self.tab_code().and_then(|c| c.active_file()) {
            None => self.render_editor_empty(cx).into_any_element(),
            #[cfg(any(windows, target_os = "macos"))]
            Some(f) if f.preview && can_preview_html(f) => self
                .editor
                .html
                .as_ref()
                .map(|html| html.clone().into_any_element())
                .unwrap_or_else(|| div().into_any_element()),
            Some(f) if f.preview => {
                let reading = f.reading.as_ref().expect("Markdown preview state").clone();
                let revision = f.edit_seq;
                reading.update(cx, |reading, cx| reading.sync(revision, cx));
                // Keep the expensive Markdown layout while the document pane
                // changes between docked and filled modes. The preview entity
                // remains the same, so GPUI can reuse its measured tree until
                // the preview notifies it about a real document or style change.
                // The cache is a separate layout root and must occupy the viewport.
                gpui::AnyView::from(reading)
                    .cached(div().size_full().style().clone())
                    .into_any_element()
            }
            Some(f) => {
                f.prepare_source(cx);
                let input = f.input.clone();
                Input::new(&input)
                    .appearance(false)
                    .scrollbar_show(ScrollbarShow::Always)
                    .editor_style(crate::ui::editor_theme::current(cx))
                    .font_family(crate::ui::editor_theme::font_family(cx))
                    .text_size(px(cx
                        .global::<crate::core::config::Config>()
                        .editor_font_size))
                    .line_height(gpui::relative(
                        cx.global::<crate::core::config::Config>()
                            .editor_line_height,
                    ))
                    .size_full()
                    .into_any_element()
            }
        };
        let conflict_banner = self
            .tab_code()
            .and_then(|c| c.active_file())
            .filter(|f| f.conflict)
            .map(|_| self.render_editor_conflict_banner(cx));

        let header = chrome
            .renders_own_header()
            .then(|| self.render_editor_header(chrome, window, cx));
        let editor_col = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(header)
            .child(self.render_editor_breadcrumb(window, cx))
            .when_some(conflict_banner, |this, b| this.child(b))
            .child(
                div()
                    .debug_selector(|| "editor-body".into())
                    .flex_1()
                    .min_h_0()
                    .child(body),
            );

        // The panel's own paint is the same either way; only the box is not.
        // Filling the workspace means stopping the window's translucency and
        // repainting the theme image the root's copy now sits under; docking
        // means sitting in the same plane as the right panel, which the column
        // wrapper has already painted.
        let shell = v_flex()
            .id("code-panel")
            .debug_selector(|| "code-panel".into());
        let shell = match chrome {
            DocumentChrome::Fill => shell
                .absolute()
                .inset_0()
                .occlude()
                // Opaque on purpose: this overlay covers the whole workspace
                // (everything but the detail panel) and an open file must never
                // let the window translucency / backdrop material show through
                // it. The preset's gradient fill is preserved, just with
                // alpha 1 — the same paint the settings overlay uses. The
                // theme background image is repainted on top of it, since the
                // root's copy now sits below this fill.
                .bg(crate::ui::theme::overlay_background(cx))
                .children(crate::ui::app::overlay_surface_layers(cx)),
            DocumentChrome::Dock | DocumentChrome::DockHoisted => shell.size_full().min_w_0(),
        };
        let shell = shell.when(
            !chrome.is_dock() && self.document_header_below_chrome(window, cx),
            |shell| shell.pt(px(crate::ui::app::TITLE_BAR_HEIGHT)),
        );
        Some(
            shell
                .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "escape" {
                        this.toggle_code_panel(window, cx);
                    }
                }))
                .child(h_flex().flex_1().min_h_0().w_full().child(editor_col))
                .when(
                    self.tab_code()
                        .and_then(|code| code.active_file())
                        .is_some_and(|file| !file.preview),
                    |shell| shell.child(self.render_code_status_bar(window, cx)),
                )
                .into_any_element(),
        )
    }

    /// The editor header alone, for the strip above a docked column.
    pub(crate) fn render_editor_header_only(
        &self,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.render_editor_header(chrome, window, cx)
            .into_any_element()
    }

    fn render_editor_header(
        &self,
        chrome: DocumentChrome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let below_chrome = !chrome.is_dock() && self.document_header_below_chrome(window, cx);
        // Reserve space only when macOS traffic lights share this header.
        let lead = if cfg!(target_os = "macos")
            && !self.left_panel_open(cx)
            && !chrome.is_dock()
            && !below_chrome
        {
            crate::ui::app::TITLE_BAR_LEAD
        } else {
            0.
        };
        let row = h_flex().id("editor-header");
        let row = if chrome.header_is_title_strip() && !below_chrome {
            crate::ui::app::title_bar_drag(row, "editor-header", window, cx)
        } else {
            row
        };
        let menu_app = cx.entity().downgrade();
        let surface = cx.global::<crate::ui::presets::Surfaces>().window;
        let content_background = surface.base;
        let inactive_tab = surface.hover;
        let commit_document = self.has_commit_document();
        let commit_front =
            commit_document && self.document_front() == Some(crate::ui::app::OverlayTop::Diff);
        let header_focus = commit_document.then(|| {
            self.tabs[self.active]
                .diff_overlay
                .as_ref()
                .unwrap()
                .document_header_focus
                .clone()
        });
        let font = gpui::font(cx.theme().font_family.clone());
        let rem = window.rem_size().as_f32();
        let file_count = self.tab_code().map_or(0, |code| code.files.len());
        row.flex_none()
            .when_some(header_focus, |row, handle| row.track_focus(&handle))
            .min_w_0()
            .h(px(crate::ui::app::TITLE_BAR_HEIGHT))
            .items_center()
            .bg(gpui::rgb(inactive_tab))
            .gap_1p5()
            .pl(px(lead))
            .pr(px(crate::ui::app::tile_trailing_inset()))
            .when(
                chrome.renders_own_header() && !self.right_panel_open(cx) && !below_chrome,
                |row| {
                    row.pr(px(crate::ui::app::tile_trailing_inset()
                        + crate::ui::tab_strip::trailing_chrome_w()))
                },
            )
            // The rail's bottom hairline lives under the tabs: the selected
            // tab's sheet covers it and joins the content below, while the
            // empty rail keeps the edge Windows Terminal draws.
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(1.))
                    .bg(cx.theme().border),
            )
            .child(
                h_flex()
                    .id("editor-file-tabs")
                    .debug_selector(|| "editor-file-tabs".into())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .bg(gpui::rgb(inactive_tab))
                    .pt(px(FILE_TAB_INSET))
                    .overflow_x_scroll()
                    .when_some(self.tab_code(), |strip, code| {
                        strip.track_scroll(&code.tab_scroll).children(
                            code.files.iter().enumerate().map(|(ix, file)| {
                                let id = file.input.entity_id();
                                let name = file.label();
                                let text_width = crate::ui::tab_strip::measure_text(
                                    window.text_system(),
                                    &font,
                                    rem * FILE_TAB_TEXT,
                                    &name,
                                );
                                // Match the row's padding, gaps, icon, close target and
                                // separator column.
                                let chrome_width = rem * 1.75
                                    + crate::ui::file_icons::ROW_ICON
                                    + crate::ui::tab_strip::MIN_TARGET
                                    + 1.
                                    + if file.dirty { 6. + rem * 0.375 } else { 0. };
                                let tab_width = (text_width + chrome_width).ceil().min(260.);
                                // The separator beside the selected tab is
                                // hidden, Windows Terminal style.
                                let front_ix = if commit_front {
                                    code.files.len()
                                } else {
                                    code.active
                                };
                                let full_path = crate::ui::path_display::readable_path(
                                    &file.path,
                                    file.host.id(),
                                )
                                .display()
                                .to_string();
                                h_flex()
                                    .id(("editor-file-tab", id))
                                    .debug_selector(move || format!("editor-file-tab-{ix}"))
                                    .occlude()
                                    .flex_none()
                                    .h_full()
                                    .w(px(tab_width))
                                    .max_w_full()
                                    .when(ix == 0, |tab| tab.ml(px(FILE_TAB_LEAD)))
                                    .px_2()
                                    .gap_1p5()
                                    .rounded_tl(px(FILE_TAB_RADIUS))
                                    .rounded_tr(px(FILE_TAB_RADIUS))
                                    .text_size(rems(FILE_TAB_TEXT))
                                    .cursor_pointer()
                                    .when(ix == code.active && !commit_front, |tab| {
                                        tab.bg(gpui::rgb(content_background))
                                            .text_color(gpui::rgb(surface.text_selected))
                                    })
                                    .when(ix != code.active || commit_front, |tab| {
                                        tab.text_color(gpui::rgb(surface.text_resting))
                                            .hover(|tab| tab.bg(gpui::rgb(surface.selected)))
                                    })
                                    .when(!(ix == front_ix || ix + 1 == front_ix), |tab| {
                                        // A short hairline, like Windows
                                        // Terminal's, that disappears beside
                                        // the selected tab.
                                        tab.child(
                                            div()
                                                .absolute()
                                                .top(px(FILE_TAB_SEP_INSET))
                                                .bottom(px(FILE_TAB_SEP_INSET))
                                                .right_0()
                                                .w(px(1.))
                                                .bg(cx.theme().border),
                                        )
                                    })
                                    .tooltip(move |window, cx| {
                                        gpui_component::tooltip::Tooltip::new(full_path.clone())
                                            .build(window, cx)
                                    })
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        if let Some(code) = app.tab_code_mut()
                                            && let Some(ix) = code
                                                .files
                                                .iter()
                                                .position(|file| file.input.entity_id() == id)
                                        {
                                            code.active = ix;
                                            code.tab_scroll.scroll_to_item(ix);
                                        }
                                        if let Some(tab) = app.tabs.get_mut(app.active) {
                                            tab.overlay_top = crate::ui::app::OverlayTop::Code;
                                            if let Some(code) = tab.code.as_mut() {
                                                code.visible = true;
                                            }
                                        }
                                        app.focus_editor(window, cx);
                                        cx.notify();
                                    }))
                                    .child(
                                        div().flex_none().child(
                                            crate::ui::file_icons::FileIcon::for_file(&name)
                                                .render(
                                                    px(crate::ui::file_icons::ROW_ICON),
                                                    window,
                                                ),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || {
                                                format!("editor-file-name-{ix}")
                                            })
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(name),
                                    )
                                    .when(file.dirty, |tab| {
                                        tab.child(
                                            div()
                                                .flex_none()
                                                .size(px(6.))
                                                .rounded_full()
                                                .bg(cx.theme().warning),
                                        )
                                    })
                                    .child(
                                        Button::new(("editor-file-close", id))
                                            .debug_selector(move || {
                                                format!("editor-file-close-{ix}")
                                            })
                                            .icon(IconName::Close)
                                            .ghost()
                                            .xsmall()
                                            .tooltip(t(L10nKey::Close))
                                            .flex_shrink_0()
                                            .w(px(crate::ui::tab_strip::MIN_TARGET))
                                            .h(px(crate::ui::tab_strip::MIN_TARGET))
                                            .on_click(cx.listener(move |app, _, window, cx| {
                                                cx.stop_propagation();
                                                if let Some(ix) = app.tab_code().and_then(|code| {
                                                    code.files.iter().position(|file| {
                                                        file.input.entity_id() == id
                                                    })
                                                }) {
                                                    app.editor_close_file(ix, window, cx);
                                                }
                                            })),
                                    )
                            }),
                        )
                    })
                    .when(commit_document, |strip| {
                        strip.child(
                            h_flex()
                                .id("editor-commit-tab")
                                .debug_selector(|| "editor-commit-tab".into())
                                .occlude()
                                .flex_none()
                                .h_full()
                                .w(px(168.))
                                .max_w_full()
                                .when(file_count == 0, |tab| tab.ml(px(FILE_TAB_LEAD)))
                                .px_2()
                                .gap_1p5()
                                .rounded_tl(px(FILE_TAB_RADIUS))
                                .rounded_tr(px(FILE_TAB_RADIUS))
                                .text_size(rems(FILE_TAB_TEXT))
                                .cursor_pointer()
                                .when(commit_front, |tab| tab.bg(gpui::rgb(content_background)))
                                .when(!commit_front, |tab| {
                                    tab.hover(|tab| tab.bg(gpui::rgb(surface.selected)))
                                })
                                .when(!commit_front, |tab| {
                                    // The commit tab keeps its rail-end
                                    // hairline until it becomes the selected
                                    // sheet.
                                    tab.child(
                                        div()
                                            .absolute()
                                            .top(px(FILE_TAB_SEP_INSET))
                                            .bottom(px(FILE_TAB_SEP_INSET))
                                            .right_0()
                                            .w(px(1.))
                                            .bg(cx.theme().border),
                                    )
                                })
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(|app, _, window, cx| {
                                    if let Some(tab) = app.tabs.get_mut(app.active)
                                        && let Some(overlay) = &tab.diff_overlay
                                    {
                                        let handle = overlay.focus_handle.clone();
                                        tab.overlay_top = crate::ui::app::OverlayTop::Diff;
                                        window.focus(&handle, cx);
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    gpui::svg()
                                        .path("icons/git-commit.svg")
                                        .size(px(crate::ui::file_icons::ROW_ICON)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(t(L10nKey::ScmCommitChanges)),
                                )
                                .child(
                                    Button::new("editor-commit-close")
                                        .debug_selector(|| "editor-commit-close".into())
                                        .icon(IconName::Close)
                                        .ghost()
                                        .xsmall()
                                        .tooltip(t(L10nKey::Close))
                                        .flex_none()
                                        .w(px(crate::ui::tab_strip::MIN_TARGET))
                                        .h(px(crate::ui::tab_strip::MIN_TARGET))
                                        .on_click(cx.listener(|app, _, window, cx| {
                                            cx.stop_propagation();
                                            app.close_diff_overlay(window, cx);
                                        })),
                                ),
                        )
                    })
                    // gpui measures the rail's scroll range from the children's
                    // bounds, which miss the leading margin on the first tab; a
                    // trailing lead of the same width restores it, so revealing
                    // a right-edge tab never clips its right side.
                    .child(div().flex_none().w(px(FILE_TAB_LEAD))),
            )
            .when(
                commit_front
                    && self.tabs[self.active]
                        .diff_overlay
                        .as_ref()
                        .is_some_and(crate::ui::diff_overlay::commit_has_diff_actions),
                |row| {
                    row.child(self.diff_document_actions(
                        self.tabs[self.active].diff_overlay.as_ref().unwrap(),
                        cx,
                    ))
                },
            )
            .child(self.document_fill_button(cx))
            .child(
                div().occlude().flex_shrink_0().child(
                    crate::ui::tab_strip::chrome_tile_sized(
                        Button::new("editor-panel-close").icon(Icon::new(IconName::Close)),
                        crate::ui::app::TILE_SIZE,
                        crate::ui::app::TILE_GLYPH_LINE,
                        false,
                        cx,
                    )
                    .rounded_lg()
                    .tooltip(t(L10nKey::EditorBackToTerminal))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.document_front() == Some(crate::ui::app::OverlayTop::Diff) {
                            this.close_diff_overlay(window, cx);
                        } else {
                            this.toggle_code_panel(window, cx);
                        }
                    })),
                ),
            )
            .context_menu(move |menu, _window, cx| {
                Tty7App::document_header_menu(menu, &menu_app, cx)
            })
    }

    fn render_editor_breadcrumb(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let code = self.tab_code();
        let active = code.and_then(|c| c.active_file());
        let name = active.map(|f| f.label());
        let path = code.and_then(|c| c.active_display_path(self.spawn_host(cx)));
        let directory = path
            .as_deref()
            .and_then(Path::parent)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| {
                let directory = parent
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" › ");
                format!("{directory} › ")
            });
        let full_path = active.map(|f| {
            SharedString::from(
                crate::ui::path_display::readable_path(&f.path, f.host.id())
                    .display()
                    .to_string(),
            )
        });
        let is_html = active.is_some_and(can_preview_html);
        let preview = active.is_some_and(|file| file.preview);
        let has_preview = is_html || active.is_some_and(|file| file.reading.is_some());
        #[cfg(any(windows, target_os = "macos"))]
        let html_loading = is_html && self.editor.html_loading;
        #[cfg(not(any(windows, target_os = "macos")))]
        let html_loading = false;
        h_flex()
            .id("editor-breadcrumb-row")
            .debug_selector(|| "editor-breadcrumb-row".into())
            .flex_none()
            .min_w_0()
            .h(px(crate::ui::app::SECONDARY_BAR_HEIGHT))
            .items_center()
            .gap_1()
            .px_2()
            .bg(gpui::rgb(
                cx.global::<crate::ui::presets::Surfaces>().window.base,
            ))
            .child(
                div()
                    .id("editor-document-path")
                    .debug_selector(|| "editor-document-path".into())
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_sm()
                    .when_some(full_path, |this, path| {
                        this.tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(path.clone()).build(window, cx)
                        })
                    })
                    .child(
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .when_some(directory, |row, directory| {
                                row.child(
                                    div()
                                        .id("editor-document-directories")
                                        .debug_selector(|| "editor-document-directories".into())
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(directory),
                                )
                            })
                            .when_some(name.as_ref(), |row, name| {
                                row.child(
                                    div()
                                        .id("editor-document-icon")
                                        .debug_selector(|| "editor-document-icon".into())
                                        .flex_none()
                                        .child(
                                            crate::ui::file_icons::FileIcon::for_file(name).render(
                                                px(crate::ui::file_icons::ROW_ICON),
                                                window,
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("editor-document-name")
                                        .debug_selector(|| "editor-document-name".into())
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(name.clone()),
                                )
                            })
                            .when(name.is_none(), |row| {
                                row.child(
                                    div()
                                        .id("editor-document-name")
                                        .debug_selector(|| "editor-document-name".into())
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t(L10nKey::EditorNoFileOpen)),
                                )
                            }),
                    ),
            )
            .when(has_preview, |row| {
                row.child(self.render_editor_preview_toggle(preview, cx))
            })
            .when(is_html, |this| {
                this.child(
                    Button::new("status-html-browser")
                        .icon(IconName::Globe)
                        .ghost()
                        .xsmall()
                        .w(px(crate::ui::tab_strip::MIN_TARGET))
                        .h(px(crate::ui::tab_strip::MIN_TARGET))
                        .tooltip(t(L10nKey::PanelOpenInBrowser))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.editor_open_html_browser(window, cx)
                        })),
                )
            })
            .when(is_html && preview, |this| {
                this.child(
                    Button::new("status-html-refresh")
                        .debug_selector(|| "status-html-refresh".into())
                        .icon(Icon::default().path("icons/refresh.svg"))
                        .loading(html_loading)
                        .ghost()
                        .xsmall()
                        .w(px(crate::ui::tab_strip::MIN_TARGET))
                        .h(px(crate::ui::tab_strip::MIN_TARGET))
                        .tooltip(t(L10nKey::Reload))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.editor_refresh_html(window, cx)),
                        ),
                )
            })
    }

    fn render_editor_preview_toggle(&self, preview: bool, cx: &mut Context<Self>) -> AnyElement {
        crate::ui::tab_strip::chrome_tile_sized(
            Button::new("editor-preview-toggle")
                .debug_selector(|| "editor-preview-toggle".into())
                .icon(if preview {
                    IconName::Eye
                } else {
                    IconName::EyeOff
                }),
            crate::ui::tab_strip::MIN_TARGET,
            14.,
            false,
            cx,
        )
        .tooltip(t(if preview {
            L10nKey::EditorEdit
        } else {
            L10nKey::EditorPreview
        }))
        .on_click(cx.listener(|app, _, window, cx| app.editor_toggle_preview(window, cx)))
        .into_any_element()
    }

    fn render_code_status_bar(&self, _window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let code = self.tab_code();
        let muted = cx.theme().muted_foreground;
        let active = code.and_then(|c| c.active_file());
        let language = active.map(|f| language_for_path(&f.path));
        let cursor: Option<SharedString> = active.filter(|f| !f.preview).map(|f| {
            let pos = f.input.read(cx).cursor_position();
            t_fmt(
                L10nKey::EditorLnCol,
                &[
                    ("line", &(pos.line + 1).to_string()),
                    ("column", &(pos.character + 1).to_string()),
                ],
            )
            .into()
        });
        let wrap: Option<bool> = active.filter(|f| !f.preview).map(|f| f.wrap);

        h_flex()
            .debug_selector(|| "editor-status-bar".into())
            .flex_none()
            .w_full()
            .h(px(26.))
            .bg(cx.theme().background)
            .items_center()
            .gap_1()
            .px_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(muted)
            .child(div().flex_1().min_w_0().truncate().children(language))
            .when_some(wrap, |this, wrap| {
                this.child(
                    crate::ui::tab_strip::chrome_tile_sized(
                        Button::new("status-wrap")
                            .debug_selector(|| "status-wrap".into())
                            .icon(Icon::default().path("icons/wrap-text.svg")),
                        crate::ui::tab_strip::MIN_TARGET,
                        14.,
                        wrap,
                        cx,
                    )
                    .tooltip(if wrap {
                        t(L10nKey::EditorWrapOn)
                    } else {
                        t(L10nKey::EditorWrapOff)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        let Some(code) = this.tab_code_mut() else {
                            return;
                        };
                        let ix = code.active;
                        if let Some(f) = code.files.get_mut(ix) {
                            f.wrap = !f.wrap;
                            let wrap = f.wrap;
                            f.input.clone().update(cx, |st, cx| {
                                st.set_soft_wrap(wrap, window, cx);
                            });
                            cx.notify();
                        }
                    })),
                )
            })
            .when_some(cursor, |this, t| {
                this.child(div().flex_none().whitespace_nowrap().child(t))
            })
    }

    fn render_editor_empty(&self, cx: &Context<Self>) -> gpui::Div {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Icon::new(IconName::File)
                    .large()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(crate::ui::i18n::t(
                        crate::ui::i18n::L10nKey::OpenFileFromTree,
                    )),
            )
    }

    fn render_editor_conflict_banner(&self, cx: &mut Context<Self>) -> AnyElement {
        let tab_ix = self.active;
        let ix = self.tab_code().map(|c| c.active).unwrap_or(0);
        h_flex()
            .flex_none()
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .bg(cx.theme().warning.opacity(0.15))
            .border_b_1()
            .border_color(cx.theme().border)
            .text_sm()
            .child(div().flex_1().child(crate::ui::i18n::t(
                crate::ui::i18n::L10nKey::FileChangedOnDisk,
            )))
            .child(
                Button::new("editor-conflict-reload")
                    .label(crate::ui::i18n::t(crate::ui::i18n::L10nKey::Reload))
                    .small()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.editor_reload_from_disk(tab_ix, ix, window, cx);
                    })),
            )
            .child(
                Button::new("editor-conflict-keep")
                    .label(crate::ui::i18n::t(crate::ui::i18n::L10nKey::KeepMine))
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _w, cx| {
                        if let Some(f) = this.tab_code_mut().and_then(|c| c.files.get_mut(ix)) {
                            f.conflict = false;
                            cx.notify();
                        }
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    fn packed(color: gpui::Hsla) -> u32 {
        let color = crate::terminal::palette::hsla_to_rgb(color);
        (color.r as u32) << 16 | (color.g as u32) << 8 | color.b as u32
    }

    #[gpui::test]
    fn editor_search_highlights_follow_the_resolved_editor_palette(cx: &mut TestAppContext) {
        use crate::ui::{editor_theme, presets};
        let (app, mut vcx) = markdown_window(cx);
        for preset in presets::builtins() {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(&preset.id, window, cx);
                let base = editor_theme::current(cx);
                let background = packed(base.highlight_theme.style.editor_background.unwrap());
                let foreground = packed(base.highlight_theme.style.editor_foreground.unwrap());
                for accent in [preset.neutrals().accent, 0x434750, background] {
                    cx.set_global(presets::ActiveAccent(accent));
                    let style = editor_theme::current(cx);
                    let hit = packed(style.search_match);
                    let active = packed(style.search_match_active);
                    assert!(
                        presets::contrast(hit, active) >= 1.2,
                        "{}: current and other matches must differ",
                        preset.id
                    );
                    assert!(presets::contrast(background, active) >= 1.89);
                    assert_eq!(style.search_match.a, 1.0);
                    assert_eq!(style.search_match_active.a, 1.0);
                    for fill in [hit, active] {
                        assert!(presets::contrast(foreground, fill) >= 2.95);
                    }
                    assert_eq!(style.selection, base.selection);
                    assert_eq!(style.caret, base.caret);
                    assert_eq!(style.highlight_theme, base.highlight_theme);
                    assert_eq!(style.search_match, base.search_match);
                }
            });
        }
    }

    fn markdown_window(cx: &mut TestAppContext) -> (Entity<Tty7App>, VisualTestContext) {
        let (app, mut vcx) = crate::ui::app::test_window::harness(cx);
        app.update_in(&mut vcx, |app, _, cx| {
            crate::ui::markdown_preview::init(Default::default(), cx);
            app.tabs
                .push(crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty));
            app.active = app.tabs.len() - 1;
            cx.notify();
        });
        (app, vcx)
    }

    #[gpui::test]
    fn commit_changes_is_a_separate_document_tab_beside_open_files(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/README.md".into(),
                "# Preserve this document\n".into(),
                None,
                window,
                cx,
            );
            app.tabs[app.active].document_layout = Some(crate::core::config::DocumentLayout::Fill);
            app.open_diff_overlay(
                HostId::LOCAL,
                "/work".into(),
                crate::terminal::git_diff::DiffSource::Commit {
                    rev: "0123456789abcdef".into(),
                    label: Some(crate::terminal::git_diff::CommitLabel {
                        subject: "fix(ui): open a commit document".into(),
                        author: "Ada".into(),
                        at: 0,
                    }),
                },
                None,
                window,
                cx,
            );
        });
        // The real background probe must finish before this test installs its
        // own snapshots; parking the GPUI executor alone does not wait for its
        // Git process, and a late probe would overwrite them.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            vcx.run_until_parked();
            if app.update_in(&mut vcx, |app, _, _| {
                !app.tabs[app.active]
                    .diff_overlay
                    .as_ref()
                    .expect("the commit overlay is open")
                    .loading
            }) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "diff probe did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        vcx.run_until_parked();
        let commit_tab = vcx
            .debug_bounds("editor-commit-tab")
            .expect("a commit has its own document tab");
        let breadcrumb = vcx
            .debug_bounds("commit-document-breadcrumb")
            .expect("the commit document has a breadcrumb row");
        let details = vcx.debug_bounds("diff-commit-details").unwrap();
        let title_height = vcx.debug_bounds("diff-commit-title").unwrap().size.height;
        assert!(commit_tab.bottom() <= breadcrumb.top() && breadcrumb.bottom() <= details.top());
        let fill = vcx.debug_bounds("document-fill-toggle").unwrap();
        assert!(
            fill.bottom() <= breadcrumb.top(),
            "layout controls belong to the tab strip"
        );
        assert!(
            vcx.debug_bounds("commit-diff-toolbar").is_none(),
            "diff controls must not take a row"
        );
        app.update(&mut vcx, |app, cx| {
            let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
            overlay.load = crate::ui::diff_overlay::DiffLoad::Ready(std::sync::Arc::new(
                crate::terminal::git_diff::DiffSnapshot::default(),
            ));
            cx.notify();
        });
        vcx.run_until_parked();
        assert!(
            vcx.debug_bounds("diff-view-toggle").is_none(),
            "empty diff has no view toggle"
        );
        app.update(&mut vcx, |app, cx| {
            let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
            let file = crate::terminal::git_diff::synthesize_added(
                "fixture.rs",
                b"hello\n",
                &crate::terminal::git_diff::DiffBudget::SINGLE_FILE,
            );
            overlay.load = crate::ui::diff_overlay::DiffLoad::Ready(std::sync::Arc::new(
                crate::terminal::git_diff::DiffSnapshot {
                    files: vec![file],
                    source: overlay.source.clone(),
                    ..Default::default()
                },
            ));
            cx.notify();
        });
        vcx.run_until_parked();
        // `debug_bounds` reads the last painted frame; settling effects does
        // not paint one, so draw before looking (as the settings tests do).
        // Left implicit, this lookup failed on CI runners under load.
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        vcx.run_until_parked();
        let toggle = vcx
            .debug_bounds("diff-view-toggle")
            .expect("text diff has a view toggle");
        assert!(
            toggle.bottom() <= breadcrumb.top() && toggle.right() <= fill.left(),
            "diff toggle is in the tab strip before layout actions"
        );
        app.update(&mut vcx, |app, cx| {
            let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
            if let crate::ui::diff_overlay::DiffLoad::Ready(snapshot) = &mut overlay.load {
                std::sync::Arc::make_mut(snapshot).files[0].binary = true;
            }
            cx.notify();
        });
        vcx.run_until_parked();
        assert!(
            vcx.debug_bounds("diff-view-toggle").is_none(),
            "binary-only diff has no view toggle"
        );
        app.update(&mut vcx, |app, cx| {
            let overlay = app.tabs[app.active].diff_overlay.as_mut().unwrap();
            if let crate::ui::diff_overlay::DiffLoad::Ready(snapshot) = &mut overlay.load {
                std::sync::Arc::make_mut(snapshot).files[0].binary = false;
            }
            cx.notify();
        });
        vcx.run_until_parked();
        let divider = vcx.debug_bounds("commit-details-resize").unwrap();
        let start = divider.center();
        let end = start - gpui::point(px(0.), px(50.));
        vcx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.run_until_parked();
        let resized = vcx.debug_bounds("diff-commit-details").unwrap();
        assert!((details.size.height - resized.size.height - px(50.)).abs() <= px(2.));
        vcx.simulate_keystrokes("down");
        vcx.run_until_parked();
        let adjusted = vcx.debug_bounds("diff-commit-details").unwrap();
        assert!(
            adjusted.size.height > resized.size.height,
            "focused divider supports keyboard adjustment"
        );
        let readme = vcx.debug_bounds("editor-file-tab-0").unwrap();
        vcx.simulate_click(readme.center(), gpui::Modifiers::none());
        app.read_with(&vcx, |app, _| {
            assert_eq!(app.document_front(), Some(crate::ui::app::OverlayTop::Code));
            assert_eq!(
                app.tab_code().unwrap().active_file().unwrap().path,
                Path::new("/work/README.md")
            );
            assert!(
                app.tabs[app.active].diff_overlay.is_some(),
                "changing documents retains the commit page"
            );
        });
        vcx.run_until_parked();
        assert!(
            vcx.debug_bounds("diff-view-toggle").is_none(),
            "ordinary files hide the diff toggle"
        );
        assert_eq!(
            vcx.debug_bounds("document-fill-toggle").unwrap(),
            fill,
            "layout action stays put when switching documents"
        );
        let commit_tab = vcx.debug_bounds("editor-commit-tab").unwrap();
        vcx.simulate_click(commit_tab.center(), gpui::Modifiers::none());
        assert_eq!(
            app.read_with(&vcx, |app, _| app.document_front()),
            Some(crate::ui::app::OverlayTop::Diff)
        );
        vcx.run_until_parked();
        assert_eq!(
            vcx.debug_bounds("diff-commit-details").unwrap().size.height,
            adjusted.size.height,
            "switching files preserves the split"
        );
        let divider = vcx.debug_bounds("commit-details-resize").unwrap();
        vcx.simulate_click(divider.center(), gpui::Modifiers::none());
        vcx.simulate_keystrokes("end");
        vcx.run_until_parked();
        let largest = vcx.debug_bounds("diff-commit-details").unwrap();
        let divider = vcx.debug_bounds("commit-details-resize").unwrap();
        assert!(
            divider.bottom() + px(100.) <= vcx.update(|window, _| window.viewport_size().height),
            "divider leaves room for the diff"
        );
        let viewport = vcx.update(|window, _| window.viewport_size());
        vcx.simulate_resize(gpui::size(viewport.width, px(400.)));
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let clamped = vcx.debug_bounds("diff-commit-details").unwrap();
        vcx.simulate_keystrokes("up");
        vcx.run_until_parked();
        let moved = vcx.debug_bounds("diff-commit-details").unwrap();
        assert!(
            (clamped.size.height - moved.size.height - px(8.)).abs() <= px(2.),
            "the first inward arrow moves the divider after a window shrink: {clamped:?} -> {moved:?}"
        );
        vcx.simulate_resize(viewport);
        vcx.simulate_keystrokes("home");
        vcx.run_until_parked();
        let smallest = vcx.debug_bounds("diff-commit-details").unwrap();
        assert!(smallest.size.height >= px(99.) && smallest.size.height < largest.size.height);
        assert_eq!(
            vcx.debug_bounds("diff-commit-title").unwrap().size.height,
            title_height,
            "shrinking the message viewport must scroll rather than compress its contents"
        );
        let close = vcx.debug_bounds("editor-commit-close").unwrap();
        vcx.simulate_click(close.center(), gpui::Modifiers::none());
        app.read_with(&vcx, |app, _| {
            assert!(app.tabs[app.active].diff_overlay.is_none());
            assert_eq!(app.document_front(), Some(crate::ui::app::OverlayTop::Code));
            assert_eq!(app.tab_code().unwrap().files.len(), 1);
        });
        app.update_in(&mut vcx, |app, window, cx| {
            assert!(
                app.editor_has_focus(window, cx),
                "closing the commit document returns focus to the retained file"
            );
        });
        vcx.simulate_resize(gpui::size(px(1100.), px(700.)));
        for layout in [
            crate::core::config::DocumentLayout::Fill,
            crate::core::config::DocumentLayout::Dock,
        ] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.tabs[app.active].document_layout = Some(layout);
                app.open_diff_overlay(
                    HostId::LOCAL,
                    "/work".into(),
                    crate::terminal::git_diff::DiffSource::Commit {
                        rev: "keyboard-close".into(),
                        label: Some(Default::default()),
                    },
                    None,
                    window,
                    cx,
                );
            });
            vcx.run_until_parked();
            app.update_in(&mut vcx, |app, window, cx| {
                if layout == crate::core::config::DocumentLayout::Dock {
                    assert!(app.document_dock_px(window, cx).is_some());
                }
                window.focus(
                    &app.tabs[app.active]
                        .diff_overlay
                        .as_ref()
                        .unwrap()
                        .document_header_focus,
                    cx,
                );
            });
            vcx.simulate_keystrokes(if cfg!(target_os = "macos") {
                "secondary-w"
            } else {
                "secondary-shift-w"
            });
            app.read_with(&vcx, |app, _| {
                assert_eq!(
                    app.tabs.len(),
                    1,
                    "closing from the document header preserves the terminal"
                );
                assert!(app.tabs[app.active].diff_overlay.is_none());
                assert_eq!(app.tab_code().unwrap().files.len(), 1);
            });
        }
    }

    #[gpui::test]
    fn selected_commit_tab_is_revealed_after_many_open_files(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        vcx.simulate_resize(gpui::size(px(1100.), px(700.)));
        let source = crate::terminal::git_diff::DiffSource::Commit {
            rev: "many-files".into(),
            label: Some(Default::default()),
        };
        app.update_in(&mut vcx, |app, window, cx| {
            app.tabs[app.active].document_layout = Some(crate::core::config::DocumentLayout::Fill);
            for n in 0..18 {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    format!("/work/file-{n}.rs").into(),
                    "fn main() {}".into(),
                    None,
                    window,
                    cx,
                );
            }
            app.editor_activate_open(HostId::LOCAL, Path::new("/work/file-0.rs"), window, cx);
        });
        vcx.run_until_parked();
        app.update_in(&mut vcx, |app, window, cx| {
            app.open_diff_overlay(
                HostId::LOCAL,
                "/work".into(),
                source.clone(),
                None,
                window,
                cx,
            );
        });
        for _ in 0..2 {
            vcx.run_until_parked();
            let tab = vcx.debug_bounds("editor-commit-tab").unwrap();
            let strip = vcx.debug_bounds("editor-file-tabs").unwrap();
            assert!(
                tab.left() + px(1.) >= strip.left() && tab.right() <= strip.right() + px(1.),
                "the active commit tab is revealed: tab={tab:?}, strip={strip:?}"
            );
            app.update_in(&mut vcx, |app, window, cx| {
                app.editor_activate_open(HostId::LOCAL, Path::new("/work/file-0.rs"), window, cx);
            });
            vcx.run_until_parked();
            let first = vcx.debug_bounds("editor-file-tab-0").unwrap();
            vcx.simulate_click(first.center(), gpui::Modifiers::none());
            app.update_in(&mut vcx, |app, window, cx| {
                app.open_diff_overlay(
                    HostId::LOCAL,
                    "/work".into(),
                    source.clone(),
                    None,
                    window,
                    cx,
                );
            });
        }
    }

    #[gpui::test]
    fn editor_long_lines_scroll_with_the_horizontal_track(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let input = app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/long-lines.rs".into(),
                format!(
                    "const LONG_LINE: &str = \"{}END_OF_LONG_LINE\";\n",
                    "0123456789 ".repeat(40)
                ),
                None,
                window,
                cx,
            );
            app.tab_code().unwrap().active_file().unwrap().input.clone()
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let body = vcx.debug_bounds("editor-body").unwrap();
        assert_eq!(
            input.read_with(&vcx, |input, _| input.scroll_offset().x),
            px(0.)
        );
        vcx.simulate_click(
            gpui::point(body.right() - px(30.), body.bottom() - px(5.)),
            gpui::Modifiers::none(),
        );
        vcx.run_until_parked();
        assert!(
            input.read_with(&vcx, |input, _| input.scroll_offset().x) < px(-100.),
            "clicking the bottom horizontal track must reveal the long line ending"
        );
    }

    #[gpui::test]
    fn editor_binary_notice_can_be_muted_without_hiding_open_errors(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("archive.ZIP");
        let binary = dir.path().join("binary.dat");
        let invalid_text = dir.path().join("legacy.txt");
        // A large archive must use the unsupported-preview hint, not the size error.
        std::fs::File::create(&archive)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        std::fs::write(&binary, b"data\0bytes").unwrap();
        std::fs::write(&invalid_text, [0xff, 0xfe]).unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                dir.path().join("keep.rs"),
                "fn keep() {}".into(),
                None,
                window,
                cx,
            );
            app.tabs[app.active].overlay_top = crate::ui::app::OverlayTop::Diff;
        });
        for path in [&archive, &binary] {
            app.update_in(&mut vcx, |app, window, cx| {
                gpui_component::Root::read(window, cx)
                    .notification
                    .clone()
                    .update(cx, |list, cx| list.clear(window, cx));
                app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), path, window, cx);
            });
            wait_for_markdown(&mut vcx, |cx| {
                cx.update(|window, cx| {
                    !gpui_component::Root::read(window, cx)
                        .notification
                        .read(cx)
                        .notifications()
                        .is_empty()
                })
            });
            app.read_with(&vcx, |app, _| {
                assert_eq!(
                    app.tabs[app.active].overlay_top,
                    crate::ui::app::OverlayTop::Diff
                );
                assert_eq!(app.tab_code().unwrap().files.len(), 1);
            });
        }
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_binary_file_notice("another.zip", window, cx);
            assert_eq!(
                gpui_component::Root::read(window, cx)
                    .notification
                    .read(cx)
                    .notifications()
                    .len(),
                1
            );
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let mute = vcx.debug_bounds("editor-hide-binary-notice").unwrap();
        vcx.simulate_click(mute.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |_, cx| {
            assert!(
                cx.global::<Config>().show_binary_file_notice,
                "checking the box must not persist before confirmation"
            );
        });
        let confirm = vcx.debug_bounds("editor-binary-notice-ok").unwrap();
        vcx.simulate_click(confirm.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.update_in(&mut vcx, |app, window, cx| {
            assert!(!cx.global::<Config>().show_binary_file_notice);
            let list = gpui_component::Root::read(window, cx).notification.clone();
            list.update(cx, |list, cx| list.clear(window, cx));
            app.editor_binary_file_notice("muted.zip", window, cx);
            assert!(list.read(cx).notifications().is_empty());
        });
        for path in [&invalid_text, &dir.path().join("missing.zip")] {
            app.update_in(&mut vcx, |app, window, cx| {
                gpui_component::Root::read(window, cx)
                    .notification
                    .clone()
                    .update(cx, |list, cx| list.clear(window, cx));
                app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), path, window, cx);
            });
            wait_for_markdown(&mut vcx, |cx| {
                cx.update(|window, cx| {
                    !gpui_component::Root::read(window, cx)
                        .notification
                        .read(cx)
                        .notifications()
                        .is_empty()
                })
            });
        }
        app.update_in(&mut vcx, |app, window, cx| {
            gpui_component::Root::read(window, cx)
                .notification
                .clone()
                .update(cx, |list, cx| list.clear(window, cx));
            app.set_binary_file_notice(true, cx);
            app.editor_binary_file_notice("restored.zip", window, cx);
            assert!(cx.global::<Config>().show_binary_file_notice);
            assert_eq!(
                app.tabs[app.active].overlay_top,
                crate::ui::app::OverlayTop::Diff
            );
            assert_eq!(app.tab_code().unwrap().files.len(), 1);
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let confirm = vcx.debug_bounds("editor-binary-notice-ok").unwrap();
        vcx.simulate_click(confirm.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |_, cx| {
            assert!(
                cx.global::<Config>().show_binary_file_notice,
                "confirming an unchecked box only dismisses this hint"
            );
        });
    }

    #[gpui::test]
    fn document_tabs_open_in_order_switch_and_close_independently(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            for path in [
                "/work/first/guide.md",
                "/work/second/guide.md",
                "/work/main.rs",
            ] {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    path.into(),
                    "# Document\n".into(),
                    None,
                    window,
                    cx,
                );
            }
            // Reopening a file must activate it without moving its tab.
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/second/guide.md".into(),
                String::new(),
                None,
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| {
            let code = app.tab_code().unwrap();
            assert_eq!(
                code.files
                    .iter()
                    .map(|f| f.path.to_str().unwrap())
                    .collect::<Vec<_>>(),
                [
                    "/work/first/guide.md",
                    "/work/second/guide.md",
                    "/work/main.rs"
                ]
            );
            assert_eq!(code.active, 1);
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let first = vcx
            .debug_bounds("editor-file-tab-0")
            .expect("the first file has a visible tab");
        vcx.simulate_click(first.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| assert_eq!(app.tab_code().unwrap().active, 0));
        // Closing an inactive file does not activate it through event bubbling.
        let close = vcx.debug_bounds("editor-file-close-1").unwrap();
        vcx.simulate_click(close.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| {
            let code = app.tab_code().unwrap();
            assert_eq!(
                code.active_file().unwrap().path,
                Path::new("/work/first/guide.md")
            );
            assert_eq!(code.files.len(), 2);
        });
        for remaining in [1, 0] {
            let close = vcx.debug_bounds("editor-file-close-0").unwrap();
            vcx.simulate_click(close.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            app.read_with(&vcx, |app, _| {
                let code = app.tab_code().unwrap();
                assert_eq!(code.files.len(), remaining);
                assert_eq!(code.visible, remaining != 0);
            });
        }
    }

    #[gpui::test]
    fn document_tabs_close_after_their_parent_folder_is_deleted(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("deleted-folder");
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("guide.md");
        std::fs::write(&path, "# Guide\n").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), &path, window, cx);
        });
        wait_for_markdown(&mut vcx, |vcx| {
            app.read_with(vcx, |app, _| {
                app.tab_code().is_some_and(|code| code.files.len() == 1)
                    && !app.editor.watch_opening
                    && !app.editor.watch_busy
            })
        });
        let opened = app.read_with(&vcx, |app, _| {
            app.tab_code().unwrap().active_file().unwrap().path.clone()
        });
        std::fs::remove_dir_all(&folder).unwrap();
        let mut reloaded = std::pin::pin!(app.next_notification(std::time::Duration::ZERO, cx));
        // Deliver the watch event deterministically, then wait for the failed read.
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_apply_external_change(HostId::LOCAL, &opened, None, window, cx);
        });
        wait_for_markdown(&mut vcx, |_| {
            gpui::block_on(smol::future::poll_once(reloaded.as_mut())).is_some()
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let close = vcx.debug_bounds("editor-file-close-0").unwrap();
        vcx.simulate_click(close.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| {
            let code = app.tab_code().unwrap();
            assert!(
                code.files.is_empty(),
                "an unedited deleted file must close without trying to save it"
            );
            assert!(
                !code.visible,
                "closing the final file must dismiss the panel"
            );
        });
        assert!(!folder.exists());
    }

    #[gpui::test]
    fn deleted_document_keeps_real_edits_and_can_be_discarded(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edited.md");
        std::fs::write(&path, "# Original\n").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                path.clone(),
                "# Original\n".into(),
                None,
                window,
                cx,
            );
            app.editor_toggle_preview(window, cx);
        });
        vcx.run_until_parked();
        vcx.simulate_keystrokes("x");
        let edited = app.read_with(&vcx, |app, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(file.dirty);
            file.input.read(cx).text().to_string()
        });
        std::fs::remove_file(&path).unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_apply_external_change(HostId::LOCAL, &path, None, window, cx);
            assert!(app.tab_code().unwrap().active_file().unwrap().conflict);
            app.editor_reload_from_disk(app.active, 0, window, cx);
        });
        wait_for_markdown(&mut vcx, |vcx| {
            app.read_with(vcx, |app, _| {
                !app.tab_code().unwrap().active_file().unwrap().conflict
            })
        });
        app.read_with(&vcx, |app, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(file.dirty, "a failed reload must preserve real edits");
            assert_eq!(file.input.read(cx).text().to_string(), edited);
        });
        for answer in [L10nKey::Cancel, L10nKey::EditorDiscard] {
            let close = vcx.debug_bounds("editor-file-close-0").unwrap();
            vcx.simulate_click(close.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            cx.simulate_prompt_answer(crate::ui::i18n::t(answer));
            vcx.run_until_parked();
            app.read_with(&vcx, |app, _| {
                assert_eq!(
                    app.tab_code().unwrap().files.len(),
                    usize::from(answer == L10nKey::Cancel)
                );
            });
        }
        assert!(!path.exists(), "discard must not recreate the deleted file");
    }

    #[gpui::test]
    fn document_tabs_show_typical_filenames_when_the_header_has_room(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        vcx.simulate_resize(gpui::size(px(1200.), px(760.)));
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_document_layout(crate::core::config::DocumentLayout::Fill, cx);
            for name in ["migration.md", "release.md", "settings.rs"] {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    Path::new("/work").join(name),
                    String::new(),
                    None,
                    window,
                    cx,
                );
            }
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        for (filename, name_selector, tab_selector, close_selector) in [
            (
                "migration.md",
                "editor-file-name-0",
                "editor-file-tab-0",
                "editor-file-close-0",
            ),
            (
                "release.md",
                "editor-file-name-1",
                "editor-file-tab-1",
                "editor-file-close-1",
            ),
            (
                "settings.rs",
                "editor-file-name-2",
                "editor-file-tab-2",
                "editor-file-close-2",
            ),
        ] {
            let name = vcx.debug_bounds(name_selector).unwrap();
            let tab = vcx.debug_bounds(tab_selector).unwrap();
            let close = vcx.debug_bounds(close_selector).unwrap();
            let text_width = vcx.update(|window, cx| {
                crate::ui::tab_strip::measure_text(
                    window.text_system(),
                    &gpui::font(cx.theme().font_family.clone()),
                    window.rem_size().as_f32() * FILE_TAB_TEXT,
                    filename,
                )
            });
            assert!(
                name.size.width >= px(text_width - 0.5),
                "ordinary filenames must be readable instead of an ellipsis: {name_selector}: {name:?}"
            );
            assert!(name.left() > tab.left() && name.right() <= close.left());
            assert!(close.right() <= tab.right());
        }
    }

    #[gpui::test]
    fn document_tabs_sit_inset_in_the_rail_and_reveal_on_scroll(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        vcx.simulate_resize(gpui::size(px(1200.), px(760.)));
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_document_layout(crate::core::config::DocumentLayout::Fill, cx);
            for name in ["migration.md", "release.md", "settings.rs"] {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    Path::new("/work").join(name),
                    String::new(),
                    None,
                    window,
                    cx,
                );
            }
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let strip = vcx.debug_bounds("editor-file-tabs").unwrap();
        let first = vcx.debug_bounds("editor-file-tab-0").unwrap();
        assert!(
            (first.left() - (strip.left() + px(FILE_TAB_LEAD))).abs() <= px(1.),
            "the first tab keeps the rail's lead: tab={first:?}, strip={strip:?}"
        );
        for selector in [
            "editor-file-tab-0",
            "editor-file-tab-2",
            "editor-file-tab-1",
        ] {
            let tab = vcx.debug_bounds(selector).unwrap();
            vcx.simulate_click(tab.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let strip = vcx.debug_bounds("editor-file-tabs").unwrap();
            let tab = vcx.debug_bounds(selector).unwrap();
            assert!(
                (tab.top() - (strip.top() + px(FILE_TAB_INSET))).abs() <= px(1.),
                "the tab hangs below the rail's top band: tab={tab:?}, strip={strip:?}"
            );
            assert!((tab.bottom() - strip.bottom()).abs() <= px(1.));
        }

        // The inset holds when the rail scrolls: overflow the strip, reveal
        // the last document, and re-check the same geometry.
        app.update_in(&mut vcx, |app, window, cx| {
            for ix in 3..10 {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    format!("/work/document-number-{ix}.rs").into(),
                    String::new(),
                    None,
                    window,
                    cx,
                );
            }
        });
        vcx.run_until_parked();
        for _ in 0..2 {
            app.update_in(&mut vcx, |app, window, cx| {
                app.editor_activate_open(
                    HostId::LOCAL,
                    Path::new("/work/document-number-9.rs"),
                    window,
                    cx,
                );
            });
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        let strip = vcx.debug_bounds("editor-file-tabs").unwrap();
        let tab = vcx.debug_bounds("editor-file-tab-9").unwrap();
        assert!(
            tab.left() >= strip.left() && tab.right() <= strip.right(),
            "the reveal must hold: tab={tab:?}, strip={strip:?}"
        );
        assert!((tab.top() - (strip.top() + px(FILE_TAB_INSET))).abs() <= px(1.));
        assert!((tab.bottom() - strip.bottom()).abs() <= px(1.));
    }

    #[gpui::test]
    fn document_tabs_reopen_canonical_local_paths_without_duplicates(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("guide.md");
        let second = dir.path().join("main.rs");
        std::fs::write(&first, "# Guide\n").unwrap();
        std::fs::write(&second, "fn main() {}\n").unwrap();
        for path in [&first, &second, &first] {
            let canonical = std::fs::canonicalize(path).unwrap();
            app.update_in(&mut vcx, |app, window, cx| {
                app.editor_open_on_host(tty7_core::host::local::LocalHost::new(), path, window, cx);
            });
            wait_for_markdown(&mut vcx, |vcx| {
                app.read_with(vcx, |app, _| {
                    app.tab_code()
                        .and_then(|code| code.active_file())
                        .is_some_and(|file| file.path == canonical)
                })
            });
        }
        app.read_with(&vcx, |app, _| {
            let code = app.tab_code().unwrap();
            assert_eq!(
                code.files.len(),
                2,
                "ordinary and canonical paths share one file tab"
            );
            assert_eq!(code.active, 0);
            assert_eq!(code.files[0].path, std::fs::canonicalize(&first).unwrap());
            assert_eq!(code.files[1].path, std::fs::canonicalize(&second).unwrap());
        });
    }

    #[gpui::test]
    fn document_tabs_keep_modes_edits_and_protect_dirty_closes(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.md");
        let second = dir.path().join("second.md");
        std::fs::write(&first, "# First\n").unwrap();
        std::fs::write(&second, "# Second\n").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            for path in [&first, &second] {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    path.clone(),
                    std::fs::read_to_string(path).unwrap(),
                    None,
                    window,
                    cx,
                );
            }
        });
        vcx.run_until_parked();
        let first_tab = vcx.debug_bounds("editor-file-tab-0").unwrap();
        vcx.simulate_click(first_tab.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        let source = vcx.debug_bounds("editor-preview-toggle").unwrap();
        vcx.simulate_click(source.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        vcx.simulate_keystrokes("x");
        let (edited, cursor) = app.read_with(&vcx, |app, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(file.dirty);
            (
                file.input.read(cx).text().to_string(),
                file.input.read(cx).cursor_position(),
            )
        });
        let second_tab = vcx.debug_bounds("editor-file-tab-1").unwrap();
        vcx.simulate_click(second_tab.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| {
            assert!(app.tab_code().unwrap().active_file().unwrap().preview)
        });
        let first_tab = vcx.debug_bounds("editor-file-tab-0").unwrap();
        vcx.simulate_click(first_tab.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |app, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(!file.preview);
            assert_eq!(file.input.read(cx).text().to_string(), edited);
            assert_eq!(file.input.read(cx).cursor_position(), cursor);
        });
        for answer in [L10nKey::Cancel, L10nKey::Save] {
            let close = vcx.debug_bounds("editor-file-close-0").unwrap();
            vcx.simulate_click(close.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            cx.simulate_prompt_answer(crate::ui::i18n::t(answer));
            vcx.run_until_parked();
            if answer == L10nKey::Cancel {
                app.read_with(&vcx, |app, _| {
                    assert_eq!(app.tab_code().unwrap().files.len(), 2)
                });
            }
        }
        wait_for_markdown(&mut vcx, |vcx| {
            app.read_with(vcx, |app, _| app.tab_code().unwrap().files.len() == 1)
        });
        assert_eq!(std::fs::read_to_string(&first).unwrap(), edited);
        app.read_with(&vcx, |app, _| {
            assert!(app.tab_code().unwrap().active_file().unwrap().preview)
        });
        let source = vcx.debug_bounds("editor-preview-toggle").unwrap();
        vcx.simulate_click(source.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        vcx.simulate_keystrokes("x");
        let close = vcx.debug_bounds("editor-file-close-0").unwrap();
        vcx.simulate_click(close.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        cx.simulate_prompt_answer(crate::ui::i18n::t(L10nKey::EditorDiscard));
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| assert!(!app.tab_code().unwrap().visible));
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "# Second\n");
    }

    #[gpui::test]
    fn document_tabs_reveal_opened_files_when_the_strip_overflows(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        vcx.simulate_resize(gpui::size(px(560.), px(900.)));
        app.update_in(&mut vcx, |app, window, cx| {
            for ix in 0..8 {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    format!("/work/document-number-{ix}.rs").into(),
                    "fn main() {}".into(),
                    None,
                    window,
                    cx,
                );
            }
        });
        vcx.run_until_parked();
        for (path, selector) in [
            ("/work/document-number-7.rs", "editor-file-tab-7"),
            ("/work/document-number-0.rs", "editor-file-tab-0"),
        ] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.editor_open_on_host(
                    tty7_core::host::local::LocalHost::new(),
                    Path::new(path),
                    window,
                    cx,
                )
            });
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let strip = vcx.debug_bounds("editor-file-tabs").unwrap();
            let tab = vcx.debug_bounds(selector).unwrap();
            assert!(
                tab.left() >= strip.left() && tab.right() <= strip.right(),
                "active tab must be revealed: {tab:?}, strip: {strip:?}"
            );
        }
        app.read_with(&vcx, |app, _| {
            assert_eq!(app.tab_code().unwrap().files.len(), 8)
        });
    }

    #[gpui::test]
    fn document_tabs_edit_the_owning_session_when_paths_match(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let original = "fn first() {}";
        let first_session = app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/shared.rs".into(),
                original.into(),
                None,
                window,
                cx,
            );
            let first = app.active;
            app.tabs
                .push(crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty));
            app.activate(app.tabs.len() - 1, window, cx);
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/shared.rs".into(),
                original.into(),
                None,
                window,
                cx,
            );
            first
        });
        vcx.run_until_parked();
        vcx.simulate_keystrokes("x");
        app.read_with(&vcx, |app, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(
                file.dirty,
                "editing belongs to the current session's buffer"
            );
            assert_ne!(file.input.read(cx).text().to_string(), original);
        });
        app.update_in(&mut vcx, |app, window, cx| {
            app.activate(first_session, window, cx)
        });
        vcx.run_until_parked();
        app.read_with(&vcx, |app, cx| {
            let code = app.tab_code().unwrap();
            assert_eq!(code.files.len(), 1);
            let file = code.active_file().unwrap();
            assert!(
                !file.dirty,
                "another session's edits do not dirty this buffer"
            );
            assert_eq!(file.input.read(cx).text().to_string(), original);
        });
    }

    #[gpui::test]
    fn html_browser_opens_the_requested_file_instead_of_the_active_editor(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let active = dir.path().join("A.html");
        let target = dir.path().join("B 页面 with spaces.HTML");
        std::fs::write(&active, "<h1>A</h1>").unwrap();
        std::fs::write(&target, "<h1>B</h1>").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                active.clone(),
                "<h1>A</h1>".into(),
                None,
                window,
                cx,
            );
            app.tab_code_mut().unwrap().files[0].dirty = true;
            app.open_html_in_browser(&target, window, cx);
            assert_eq!(app.tab_code().unwrap().active_file().unwrap().path, active);
            assert_eq!(app.tab_code().unwrap().files.len(), 1);
        });
        let opened = cx.opened_url().expect("the target opens externally");
        #[cfg(windows)]
        assert_eq!(opened, target.to_string_lossy());
        #[cfg(not(windows))]
        assert_eq!(
            url::Url::parse(&opened).unwrap().to_file_path().unwrap(),
            target
        );
    }

    #[gpui::test]
    fn html_browser_protects_unsaved_target_in_another_tab(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("unsaved.htm");
        std::fs::write(&target, "<h1>Saved</h1>").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                target.clone(),
                "<h1>Saved</h1>".into(),
                None,
                window,
                cx,
            );
            let code = app.tab_code_mut().unwrap();
            let file = &mut code.files[code.active];
            file.input.update(cx, |input, cx| {
                input.set_value("<h1>Unsaved</h1>", window, cx);
            });
            file.dirty = true;
            app.tabs
                .push(crate::ui::app::Tab::new(crate::ui::pane::Pane::Empty));
            app.active = app.tabs.len() - 1;
            app.open_html_in_browser(&target, window, cx);
        });
        assert!(cx.opened_url().is_none(), "unsaved targets must not open");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "<h1>Saved</h1>");
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[gpui::test]
    fn html_preview_pending_open_preserves_new_unsaved_edits(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pending.html");
        std::fs::write(&path, "<h1>Saved</h1>").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                path.clone(),
                "<h1>Saved</h1>".into(),
                None,
                window,
                cx,
            );
            let code = app.tab_code_mut().unwrap();
            let file = &mut code.files[code.active];
            file.input.update(cx, |input, cx| {
                input.set_value("<h1>Unsaved</h1>", window, cx)
            });
            file.dirty = true;
            // A pending preview must protect edits even while a dialog hides it.
            app.editor_sync_html_preview(false, window, cx);
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(
                !file.preview,
                "pending preview must return to the edited source"
            );
            assert!(file.dirty);
            assert_eq!(file.input.read(cx).text().to_string(), "<h1>Unsaved</h1>");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "<h1>Saved</h1>");
        });
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[gpui::test]
    fn html_preview_column_only_link_opens_source(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("报告 with spaces.HtMl");
        std::fs::write(&path, "<h1>Saved</h1>").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                path.clone(),
                "<h1>Saved</h1>".into(),
                None,
                window,
                cx,
            );
            app.open_file_in_editor_at(&path, None, Some(5), window, cx);
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(!file.preview, "an explicit column must open source");
            assert_eq!(file.input.read(cx).cursor_position(), Position::new(0, 4));
        });
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[gpui::test]
    fn html_preview_is_default_and_source_edits_require_saving(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.html");
        let saved = "<h1>Saved</h1>";
        std::fs::write(&path, saved).unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                path.clone(),
                saved.into(),
                None,
                window,
                cx,
            );
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(can_preview_html(file));
            assert!(file.preview);
            assert!(file.reading.is_none());
            assert!(app.editor_needs_html_preview_sync());
            assert!(
                app.editor.html.is_none(),
                "the browser is created when preview is rendered"
            );
            app.editor_toggle_preview(window, cx);
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
            assert!(!app.editor_needs_html_preview_sync());
            let code = app.tab_code_mut().unwrap();
            let file = &mut code.files[code.active];
            file.input.update(cx, |input, cx| {
                input.set_value("<h1>Unsaved</h1>", window, cx)
            });
            file.dirty = true;
            app.editor_toggle_preview(window, cx);
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(!file.preview);
            assert!(file.dirty);
            assert_eq!(file.input.read(cx).text().to_string(), "<h1>Unsaved</h1>");
            assert!(app.editor.html.is_none());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
        });
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[gpui::test]
    fn html_preview_default_keeps_explicit_source_navigation_and_mode_switching(
        cx: &mut TestAppContext,
    ) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            let path = dir.path().join("index.HTM");
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                path.clone(),
                "<h1>Index</h1>".into(),
                None,
                window,
                cx,
            );
            assert!(app.tab_code().unwrap().active_file().unwrap().preview);
            assert!(app.editor_needs_html_preview_sync());
            app.apply_editor_navigation(
                HostId::LOCAL,
                &path,
                Some(EditorNavigation::Source { line: 1, column: 1 }),
                window,
                cx,
            );
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
            assert!(!app.editor_needs_html_preview_sync());
            assert!(app.editor_activate_open(HostId::LOCAL, &path, window, cx));
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
            app.editor_toggle_preview(window, cx);
            assert!(app.tab_code().unwrap().active_file().unwrap().preview);
            app.editor_toggle_preview(window, cx);
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
        });
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[gpui::test]
    fn html_preview_release_preserves_each_files_selected_mode(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            for (path, content) in [
                ("report.html", "<h1>Report</h1>"),
                ("index.HTM", "<p>Index</p>"),
                ("guide.md", "# Guide"),
            ] {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    dir.path().join(path),
                    content.into(),
                    None,
                    window,
                    cx,
                );
            }
            assert!(app.editor_activate_open(
                HostId::LOCAL,
                &dir.path().join("index.HTM"),
                window,
                cx
            ));
            app.editor_toggle_preview(window, cx);
            app.editor_release_html();
            let files = &app.tab_code().unwrap().files;
            assert_eq!(
                files.iter().filter(|file| can_preview_html(file)).count(),
                2
            );
            let report = files
                .iter()
                .find(|file| file.path == dir.path().join("report.html"))
                .unwrap();
            let index = files
                .iter()
                .find(|file| file.path == dir.path().join("index.HTM"))
                .unwrap();
            assert!(
                report.preview,
                "releasing the browser must preserve inactive preview mode"
            );
            assert!(
                !index.preview,
                "explicit source selection belongs to this file only"
            );
            let markdown = files
                .iter()
                .find(|file| file.path.extension().unwrap() == "md")
                .unwrap();
            assert!(markdown.preview, "Markdown owns a separate native preview");
            assert!(markdown.reading.is_some());
        });
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[gpui::test]
    fn html_preview_failure_only_resets_the_active_file(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            for path in ["first.html", "second.html"] {
                app.editor_install_file(
                    tty7_core::host::local::LocalHost::new(),
                    dir.path().join(path),
                    "<h1>Saved</h1>".into(),
                    None,
                    window,
                    cx,
                );
            }
            app.editor_fallback_html_source();
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
            assert!(!app.editor_needs_html_preview_sync());
            assert!(app.editor_activate_open(
                HostId::LOCAL,
                &dir.path().join("first.html"),
                window,
                cx
            ));
            assert!(app.tab_code().unwrap().active_file().unwrap().preview);
            assert!(app.editor_needs_html_preview_sync());
            // The native browser is exercised separately from this test window.
            app.editor_toggle_preview(window, cx);
        });
    }

    #[gpui::test]
    fn document_paths_start_at_the_matching_project_without_absolute_prefixes(
        cx: &mut TestAppContext,
    ) {
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/second/docs/guide.md".into(),
                "# Guide\n".into(),
                None,
                window,
                cx,
            );
            let code = app.tab_code_mut().unwrap();
            code.roots = vec!["/work/first".into(), "/work/second".into()];
            assert_eq!(
                code.active_display_path(HostId::LOCAL),
                Some(PathBuf::from("second/docs/guide.md"))
            );
            code.roots.push("/work/second/docs".into());
            assert_eq!(
                code.active_display_path(HostId::LOCAL),
                Some(PathBuf::from("docs/guide.md")),
                "the path includes the deepest matching project root"
            );
            let active = code.active;
            code.files[active].path = "/work/second/CONTEXT.md".into();
            assert_eq!(
                code.active_display_path(HostId::LOCAL),
                Some(PathBuf::from("second/CONTEXT.md")),
                "a file at the project root still shows the project name"
            );
            code.files[active].path = "/work/second/docs/guide.md".into();
            assert_eq!(
                code.active_display_path(HostId(99)),
                Some(PathBuf::from("guide.md")),
                "roots on another host must not label this buffer's path"
            );
            code.roots = vec!["/work/elsewhere".into()];
            assert_eq!(
                code.active_display_path(HostId::LOCAL),
                Some(PathBuf::from("guide.md"))
            );
            code.roots.clear();
            assert_eq!(
                code.active_display_path(HostId::LOCAL),
                Some(PathBuf::from("guide.md")),
                "an unresolved project must never expose absolute ancestors"
            );
            #[cfg(windows)]
            for path in [
                r"D:\gh-prj\ctty7\tests\fixtures\html-preview\report.html",
                r"\\?\D:\gh-prj\ctty7\tests\fixtures\html-preview\report.html",
            ] {
                // This test covers path spelling; the native browser has its
                // own real-window acceptance fixture.
                code.files[active].preview = false;
                code.files[active].path = path.into();
                code.roots.clear();
                assert_eq!(
                    code.active_display_path(HostId::LOCAL),
                    Some("report.html".into())
                );
                code.roots.push(r"D:\gh-prj\ctty7".into());
                assert_eq!(
                    code.active_display_path(HostId::LOCAL),
                    Some(r"ctty7\tests\fixtures\html-preview\report.html".into())
                );
            }
        });
    }

    #[gpui::test]
    fn document_header_shows_project_paths_after_opening_local_files(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let directory = tempfile::tempdir().unwrap();
        // CI temp directories can use symlinks, junctions or short names.
        // Resolve those aliases just as opening the file does, but keep the
        // Windows project root in ordinary drive spelling to test the
        // namespace-prefix mismatch rather than normalizing it away.
        let root = std::fs::canonicalize(directory.path()).unwrap();
        #[cfg(windows)]
        let root = PathBuf::from(root.to_str().unwrap().strip_prefix(r"\\?\").unwrap());
        let docs = root.join("docs");
        std::fs::create_dir(&docs).unwrap();
        for (relative, contents) in [
            ("README.md", "# Readme\n"),
            ("docs/阅读说明.md", "# Guide\n"),
            ("docs/example.rs", "fn main() {}\n"),
        ] {
            let path = root.join(relative);
            std::fs::write(&path, contents).unwrap();
            app.update_in(&mut vcx, |app, window, cx| {
                app.tab_code_mut_or_init().unwrap().roots = vec![root.to_path_buf()];
                app.editor_open_on_host(
                    tty7_core::host::local::LocalHost::new(),
                    &path,
                    window,
                    cx,
                );
            });
            wait_for_markdown(&mut vcx, |vcx| {
                app.read_with(vcx, |app, _| {
                    app.tab_code()
                        .unwrap()
                        .active_file()
                        .is_some_and(|file| file.path.file_name() == path.file_name())
                })
            });
            app.read_with(&vcx, |app, _| {
                let code = app.tab_code().unwrap();
                assert_eq!(
                    code.active_file().unwrap().path,
                    std::fs::canonicalize(&path).unwrap()
                );
                assert_eq!(
                    code.active_display_path(HostId::LOCAL),
                    Some(PathBuf::from(root.file_name().unwrap()).join(relative)),
                    "canonicalized local files must start their breadcrumbs at the project name"
                );
            });
            app.update_in(&mut vcx, |app, window, cx| {
                // A tree refresh can remove the matching root while the
                // canonical buffer stays open. Reopen through the real path.
                app.tab_code_mut().unwrap().roots.clear();
                app.editor_open_on_host(
                    tty7_core::host::local::LocalHost::new(), &path, window, cx,
                );
                assert_eq!(app.tab_code().unwrap().active_display_path(HostId::LOCAL),
                    Some(PathBuf::from(path.file_name().unwrap())),
                    "reopening without a project root must keep disk and parent directories out of the header");
            });
        }
    }

    #[gpui::test]
    fn document_header_places_the_file_icon_before_its_name(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/project/docs/guide.md".into(),
                "# Guide\n".into(),
                None,
                window,
                cx,
            );
            app.tab_code_mut().unwrap().roots = vec!["/work/project".into()];
        });

        for (width, layout) in [
            (1600., crate::core::config::DocumentLayout::Dock),
            (1600., crate::core::config::DocumentLayout::Fill),
            (760., crate::core::config::DocumentLayout::Dock),
            (560., crate::core::config::DocumentLayout::Fill),
        ] {
            vcx.simulate_resize(gpui::size(px(width), px(900.)));
            app.update_in(&mut vcx, |app, _, cx| app.set_document_layout(layout, cx));
            for (path, has_directories) in [
                ("/work/project/docs/guide.md", true),
                ("/work/project/docs/阅读说明.md", true),
                ("/work/project/docs/main.rs", true),
                ("/work/project/CONTEXT.md", true),
            ] {
                app.update_in(&mut vcx, |app, _, cx| {
                    let code = app.tab_code_mut().unwrap();
                    let active = code.active;
                    code.files[active].path = path.into();
                    code.files[active].preview = language_for_path(Path::new(path)) == "markdown";
                    cx.notify();
                });
                vcx.run_until_parked();
                vcx.update(|window, cx| {
                    let _ = window.draw(cx);
                });

                let path = vcx.debug_bounds("editor-document-path").unwrap();
                let icon = vcx.debug_bounds("editor-document-icon").unwrap();
                let name = vcx.debug_bounds("editor-document-name").unwrap();
                assert!(icon.left() < name.left(), "icon must precede filename");
                assert!(icon.left() >= path.left() && icon.right() <= path.right());
                assert!(name.left() < path.right(), "filename must remain visible");

                let directories = vcx.debug_bounds("editor-document-directories");
                assert_eq!(directories.is_some(), has_directories);
                if let Some(directories) = directories {
                    assert!(
                        directories.left() < icon.left(),
                        "project breadcrumb must remain visible at {width}px in {layout:?}: {directories:?} {icon:?}"
                    );
                    assert!(directories.right() <= icon.left());
                    assert!(
                        icon.left() - directories.left() < px(200.),
                        "a short directory must not push the filename to the right: {directories:?} {icon:?}"
                    );
                }
            }
        }
    }

    #[gpui::test]
    fn document_icon_controls_fit_and_work_with_long_paths(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/work/project/a-very-long-directory/another-long-directory/reading-guide-with-a-very-long-name-阅读说明.md".into(),
                "# Guide\n\nRead and edit.\n".into(),
                None,
                window,
                cx,
            );
        });
        for width in [1600., 560., 360.] {
            vcx.simulate_resize(gpui::size(px(width), px(900.)));
            vcx.run_until_parked();
            for expected_preview in [false, true] {
                vcx.update(|window, cx| {
                    let _ = window.draw(cx);
                });
                let path = vcx.debug_bounds("editor-document-path").unwrap();
                assert!(
                    path.size.width > px(20.) && path.size.height > px(8.),
                    "the document title must have visible space: {path:?}"
                );
                assert!(
                    vcx.debug_bounds("status-md-preview").is_none(),
                    "the footer toggle is removed"
                );
                let toggle = vcx
                    .debug_bounds("editor-preview-toggle")
                    .expect("a single preview icon is beside the breadcrumb");
                assert_eq!(toggle.size.width, px(crate::ui::tab_strip::MIN_TARGET));
                assert!(toggle.size.height >= px(24.));
                let row = vcx.debug_bounds("editor-breadcrumb-row").unwrap();
                assert!(toggle.left() >= path.right() && toggle.right() <= row.right());
                assert!(toggle.top() >= row.top() && toggle.bottom() <= row.bottom());
                vcx.simulate_click(toggle.center(), gpui::Modifiers::none());
                vcx.run_until_parked();
                app.read_with(&vcx, |app, _| {
                    assert_eq!(
                        app.tab_code().unwrap().active_file().unwrap().preview,
                        expected_preview
                    )
                });
                let preview = app.read_with(&vcx, |app, _| {
                    app.tab_code().unwrap().active_file().unwrap().preview
                });
                assert_eq!(preview, expected_preview);
                if preview {
                    assert!(vcx.debug_bounds("editor-status-bar").is_none());
                    assert!(vcx.debug_bounds("status-wrap").is_none());
                } else {
                    let footer = vcx.debug_bounds("editor-status-bar").unwrap();
                    let wrap = vcx.debug_bounds("status-wrap").unwrap();
                    assert!(wrap.size.width >= px(24.) && wrap.size.height >= px(24.));
                    assert!(wrap.left() >= footer.left() && wrap.right() <= footer.right());
                    for expected in [true, false] {
                        vcx.simulate_click(wrap.center(), gpui::Modifiers::none());
                        vcx.run_until_parked();
                        app.read_with(&vcx, |app, _| {
                            assert_eq!(
                                app.tab_code().unwrap().active_file().unwrap().wrap,
                                expected
                            );
                        });
                    }
                }
            }
            assert!(app.read_with(&vcx, |app, _| {
                app.tab_code().unwrap().active_file().unwrap().preview
            }));
        }
    }

    #[gpui::test]
    fn editor_palette_dismiss_restores_source_and_reading_focus(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/focus.md".into(),
                "# Focus\n".into(),
                None,
                window,
                cx,
            );
        });
        for _ in 0..2 {
            app.update_in(&mut vcx, |app, window, cx| {
                assert!(app.editor_has_focus(window, cx));
                app.open_palette("theme", window, cx);
                assert!(!app.editor_has_focus(window, cx));
                app.close_palette(window, cx);
                assert!(
                    app.editor_has_focus(window, cx),
                    "closing the palette must return the keyboard to the document"
                );
                app.editor_toggle_preview(window, cx);
            });
        }
    }

    fn wait_for_markdown(
        vcx: &mut VisualTestContext,
        mut ready: impl FnMut(&mut VisualTestContext) -> bool,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            vcx.run_until_parked();
            if ready(vcx) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Markdown operation did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    #[gpui::test]
    fn editor_theme_colors_rust_bindings_without_recoloring_module_paths(cx: &mut TestAppContext) {
        use crate::ui::editor_theme;
        use gpui_component::highlighter::SyntaxHighlighter;
        let (app, mut vcx) = markdown_window(cx);
        let source = "use std::borrow::Cow;\nmod core;\nfn make(cx: &mut App) {\n    let fonts = \"a\\n\";\n    let file: std::path::PathBuf;\n    cx.add_fonts(fonts);\n    let outer = LIMIT;\n    let generic = Some::<u8>(1);\n    let qualified = Option::Some::<u16>(1);\n    wrap! { let inner = load_fonts(); let count = OTHER_LIMIT; let maybe = Some(inner); }\n}\n";
        for (preference, variable, keyword, function, ty, foreground) in [
            ("dark", 0xe06c75, 0xc678dd, 0x61afef, 0x56b6c2, 0xabb2bf),
            ("light", 0x1f2328, 0xcf222e, 0x6639ba, 0x1f2328, 0x1f2328),
        ] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(preference, window, cx);
                let style = editor_theme::current(cx);
                let mut highlighter = SyntaxHighlighter::new(editor_theme::language("rust"));
                highlighter.update(None, &source.into(), None);
                let runs = highlighter.styles(&(0..source.len()), &style.highlight_theme);
                for (token, expected) in [
                    ("fonts", variable),
                    ("fn", keyword),
                    ("make", function),
                    ("App", ty),
                    ("std", foreground),
                    ("borrow", foreground),
                    ("core", foreground),
                    ("path", foreground),
                    ("load_fonts", function),
                    ("inner", variable),
                    (
                        "LIMIT",
                        if preference == "dark" {
                            0xd19a66
                        } else {
                            0x0550ae
                        },
                    ),
                    (
                        "OTHER_LIMIT",
                        if preference == "dark" {
                            0xd19a66
                        } else {
                            0x0550ae
                        },
                    ),
                    ("Some::<u8>", function),
                    ("Some::<u16>", function),
                    ("Some(inner)", function),
                    ("\\n", if preference == "dark" { ty } else { 0x0550ae }),
                ] {
                    let offset = source.find(token).unwrap();
                    let color = runs
                        .iter()
                        .find(|(range, _)| range.contains(&offset))
                        .and_then(|(_, run)| run.color)
                        .or(style.highlight_theme.style.editor_foreground);
                    assert_eq!(
                        color,
                        Some(gpui::rgb(expected).into()),
                        "{preference}: {token}"
                    );
                }
            });
        }
    }

    #[gpui::test]
    fn editor_theme_python_variables_match_the_palette_foreground(cx: &mut TestAppContext) {
        use crate::ui::editor_theme;
        use gpui_component::highlighter::SyntaxHighlighter;
        let (app, mut vcx) = markdown_window(cx);
        let source = "import json\nDEST = Path(__file__).parent\nroot = Path(\"icons\")\nversion = json.loads(root.read_text())\n";
        for (preference, foreground) in [("dark", 0xabb2bf), ("light", 0x1f2328)] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(preference, window, cx);
                let style = editor_theme::current(cx);
                let mut highlighter = SyntaxHighlighter::new(editor_theme::language("python"));
                highlighter.update(None, &source.into(), None);
                let runs = highlighter.styles(&(0..source.len()), &style.highlight_theme);
                for token in ["json", "DEST", "root", "version", "parent"] {
                    let offset = source.find(token).unwrap();
                    let color = runs
                        .iter()
                        .find(|(range, _)| range.contains(&offset))
                        .and_then(|(_, run)| run.color)
                        .or(style.highlight_theme.style.editor_foreground);
                    assert_eq!(
                        color,
                        Some(gpui::rgb(foreground).into()),
                        "{preference}: {token}"
                    );
                }
            });
        }
    }

    #[test]
    fn editor_theme_preserves_exact_capture_styles_and_base_fallbacks() {
        let colors: gpui_component::highlighter::SyntaxColors = serde_json::from_str(
            r##"{"variable":{"color":"#E06C75"},"variable.parameter":{"color":"#ABB2BF"},"function":{"color":"#61AFEF"},"function.builtin":{"color":"#56B6C2"}}"##,
        ).unwrap();
        for (capture, expected) in [
            ("variable.parameter", 0xabb2bf),
            ("variable.member", 0xe06c75),
            ("function.builtin", 0x56b6c2),
            ("function.method", 0x61afef),
        ] {
            assert_eq!(
                colors.style(capture).unwrap().color,
                Some(gpui::rgb(expected).into()),
                "{capture}"
            );
        }
    }

    #[gpui::test]
    fn editor_theme_matches_authored_color_samples(cx: &mut TestAppContext) {
        use crate::ui::editor_theme;
        use gpui_component::highlighter::SyntaxHighlighter;
        #[derive(serde::Deserialize)]
        struct Token {
            text: String,
            start: usize,
            end: usize,
            color: String,
        }
        #[derive(serde::Deserialize)]
        struct Sample {
            mode: String,
            language: String,
            source: String,
            tokens: Vec<Token>,
        }
        let mut samples: Vec<Sample> = serde_json::from_str(include_str!(
            "../../assets/editor-themes/tests/textmate-colors.json"
        ))
        .unwrap();
        samples.extend(
            serde_json::from_str::<Vec<Sample>>(include_str!(
                "../../assets/editor-themes/tests/github-light-colors.json"
            ))
            .unwrap(),
        );
        samples.extend(
            serde_json::from_str::<Vec<Sample>>(include_str!(
                "../../assets/editor-themes/tests/github-website-colors.json"
            ))
            .unwrap(),
        );
        let (app, mut vcx) = markdown_window(cx);
        let mut failures = Vec::new();
        for sample in samples {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(&sample.mode, window, cx);
                let style = editor_theme::current(cx);
                // The registry owns static language names, just like path detection.
                let language = gpui_component::highlighter::Language::all()
                    .find(|language| language.name() == sample.language)
                    .unwrap()
                    .name();
                let alias = editor_theme::language(language);
                let mut highlighter = SyntaxHighlighter::new(alias);
                if highlighter.language().as_ref() != alias {
                    failures.push(format!("{language} query must compile"));
                    return;
                }
                highlighter.update(None, &sample.source.as_str().into(), None);
                let runs = highlighter.styles(&(0..sample.source.len()), &style.highlight_theme);
                for token in &sample.tokens {
                    assert_eq!(
                        &sample.source[token.start..token.end],
                        token.text,
                        "{} {}: reference token range",
                        sample.mode,
                        language
                    );
                    let expected =
                        u32::from_str_radix(token.color.trim_start_matches('#'), 16).unwrap();
                    for offset in token.start..token.end {
                        if sample.source.as_bytes()[offset].is_ascii_whitespace() {
                            continue;
                        }
                        let color = runs
                            .iter()
                            .find(|(range, _)| range.contains(&offset))
                            .and_then(|(_, run)| run.color)
                            .or(style.highlight_theme.style.editor_foreground);
                        if color != Some(gpui::rgb(expected).into()) {
                            failures.push(format!(
                                "{} {} {:?} at {}: expected {}, got {:?}",
                                sample.mode,
                                language,
                                token.text,
                                offset,
                                token.color,
                                color.map(crate::terminal::palette::hsla_to_rgb)
                            ));
                            break;
                        }
                    }
                }
            });
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[gpui::test]
    fn editor_theme_queries_compile_without_changing_shared_languages(cx: &mut TestAppContext) {
        let _ = markdown_window(cx);
        use gpui_component::highlighter::{Language, LanguageRegistry, SyntaxHighlighter};
        let registry = LanguageRegistry::singleton();
        let originals: Vec<_> = Language::all()
            .map(|language| (language.name(), registry.language(language.name()).unwrap()))
            .collect();
        let mut errors = Vec::new();
        for (name, original) in originals {
            let alias = crate::ui::editor_theme::language(name);
            let config = registry.language(alias).unwrap();
            if let Err(error) = tree_sitter::Query::new(
                &config.language,
                &format!(
                    "{}{}{}",
                    config.injections, config.locals, config.highlights
                ),
            ) {
                errors.push(format!("{name}: {error}"));
                continue;
            }
            let highlighter = SyntaxHighlighter::new(alias);
            assert_eq!(highlighter.language().as_ref(), alias, "{name}");
            assert_eq!(registry.language(name).unwrap(), original, "shared {name}");
        }
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }

    #[gpui::test]
    fn editor_theme_control_colors_match_the_authored_sources(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        for (preference, source) in [
            (
                "dark",
                include_str!("../../assets/editor-themes/upstream/OneDark.json"),
            ),
            (
                "light",
                include_str!("../../assets/editor-themes/upstream/GitHubLight.json"),
            ),
        ] {
            let source: serde_json::Value = serde_json::from_str(source).unwrap();
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(preference, window, cx);
                let style = crate::ui::editor_theme::current(cx);
                let colors = &style.highlight_theme.style;
                for (actual, key, github_key) in [
                    (
                        colors.editor_foreground.unwrap(),
                        "editor.foreground",
                        "codeMirror-fgColor",
                    ),
                    (
                        colors.editor_active_line.unwrap(),
                        "editor.lineHighlightBackground",
                        "codeMirror-activeline-bgColor",
                    ),
                    (
                        colors.editor_line_number.unwrap(),
                        "editorLineNumber.foreground",
                        "codeMirror-lineNumber-fgColor",
                    ),
                    (
                        colors.editor_active_line_number.unwrap(),
                        "editorLineNumber.activeForeground",
                        "codeMirror-fgColor",
                    ),
                    (
                        colors.editor_invisible.unwrap(),
                        "editorWhitespace.foreground",
                        "borderColor-default",
                    ),
                    (
                        style.selection,
                        "editor.selectionBackground",
                        "selection-bgColor",
                    ),
                    (
                        style.caret,
                        "editorCursor.foreground",
                        "codeMirror-cursor-fgColor",
                    ),
                    (
                        style.muted_foreground,
                        "editorLineNumber.foreground",
                        "fgColor-muted",
                    ),
                    (
                        style.border,
                        "editorIndentGuide.background",
                        "borderColor-default",
                    ),
                ] {
                    let value = if preference == "dark" {
                        &source["colors"][key]
                    } else {
                        &source["tokens"][github_key]
                    };
                    let expected: gpui::Hsla = serde_json::from_value(value.clone()).unwrap();
                    assert_eq!(actual, expected, "{preference}: {key}");
                }
            });
        }
    }

    #[gpui::test]
    fn editor_theme_preserves_language_overrides_inside_embedded_code(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        for (preference, foreground, orange, cyan, red, green, blue) in [
            (
                "dark", 0xabb2bf, 0xd19a66, 0x56b6c2, 0xe06c75, 0x98c379, 0x61afef,
            ),
            (
                "light", 0x1f2328, 0x0550ae, 0x0550ae, 0x953800, 0x0a3069, 0x6639ba,
            ),
        ] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(preference, window, cx);
                for (language, source, tokens) in [
                    ("html", "<script>let root = obj.parent + 2;</script>", vec![("root", foreground), ("=", if preference == "dark" { cyan } else { 0xcf222e }), ("parent", red), ("2", orange)]),
                    ("tsx", "const root: string = obj.parent + 2; const view = <div id=\"main\">{root}</div>;", vec![("root", orange), ("string", cyan), ("parent", red), ("div", if preference == "dark" { red } else { 0x0550ae }), ("id", orange), ("main", green)]),
                    ("javascript", "const view = <div id=\"main\">{item}</div>;", vec![("div", if preference == "dark" { red } else { 0x0550ae }), ("id", orange), ("main", green), ("item", foreground)]),
                    ("markdown", "```rust\nstruct Widget;\n```\n", vec![("Widget", if preference == "dark" { cyan } else { foreground })]),
                    ("python", "from pathlib import Path\nDEST = Path(__file__).parent\n", vec![("Path", foreground), ("Path(__file__)", blue), ("__file__", if preference == "dark" { red } else { 0x0550ae }), ("parent", foreground)]),
                ] {
                    let style = crate::ui::editor_theme::current(cx);
                    let mut highlighter = gpui_component::highlighter::SyntaxHighlighter::new(crate::ui::editor_theme::language(language));
                    highlighter.update(None, &source.into(), None);
                    let runs = highlighter.styles(&(0..source.len()), &style.highlight_theme);
                    for (token, expected) in tokens {
                        let offset = source.find(token).unwrap();
                        let color = runs.iter().find(|(range, _)| range.contains(&offset)).and_then(|(_, run)| run.color).or(style.highlight_theme.style.editor_foreground);
                        assert_eq!(color, Some(gpui::rgb(expected).into()), "{preference} {language}: {token}");
                    }
                }
            });
        }
    }

    #[gpui::test]
    fn editor_theme_highlights_cross_language_samples(cx: &mut TestAppContext) {
        use crate::ui::editor_theme;
        use gpui_component::highlighter::{LanguageRegistry, SyntaxHighlighter};
        let (app, mut vcx) = markdown_window(cx);
        let shared_rust = LanguageRegistry::singleton().language("rust").unwrap();
        editor_theme::language("rust");
        assert_eq!(
            LanguageRegistry::singleton().language("rust").unwrap(),
            shared_rust
        );
        for (preference, keyword, function, string, number) in [
            ("dark", 0xc678dd, 0x61afef, 0x98c379, 0xd19a66),
            ("light", 0xcf222e, 0x6639ba, 0x0a3069, 0x0550ae),
        ] {
            app.update_in(&mut vcx, |app, window, cx| {
                app.set_theme_follow_system(false, window, cx);
                app.set_preset(preference, window, cx);
                for (language, source, tokens) in [
                    (
                        "typescript",
                        "function greet() { return \"hello\"; }",
                        vec![
                            ("function", keyword),
                            ("greet", function),
                            ("hello", string),
                        ],
                    ),
                    (
                        "javascript",
                        "function greet() { return 42; }",
                        vec![("function", keyword), ("greet", function), ("42", number)],
                    ),
                    (
                        "python",
                        "def greet():\n    return \"hello\"",
                        vec![("def", keyword), ("greet", function), ("hello", string)],
                    ),
                    (
                        "json",
                        "{\"count\": 42, \"text\": \"hello\"}",
                        vec![("42", number), ("hello", string)],
                    ),
                ] {
                    let style = editor_theme::current(cx);
                    let mut highlighter = SyntaxHighlighter::new(editor_theme::language(language));
                    highlighter.update(None, &source.into(), None);
                    let runs = highlighter.styles(&(0..source.len()), &style.highlight_theme);
                    for (token, expected) in tokens {
                        let offset = source.find(token).unwrap();
                        let color = runs
                            .iter()
                            .find(|(range, _)| range.contains(&offset))
                            .and_then(|(_, run)| run.color);
                        assert_eq!(
                            color,
                            Some(gpui::rgb(expected).into()),
                            "{preference} {language}: {token}"
                        );
                    }
                }
            });
        }
    }

    #[gpui::test]
    fn editor_theme_background_follows_presets_and_preserves_edits(cx: &mut TestAppContext) {
        use crate::core::config::Config;
        use crate::ui::editor_theme;
        let (app, mut vcx) = markdown_window(cx);
        let original = "fn main() {}\n".repeat(150);
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        app.update_in(&mut vcx, |app, window, cx| {
            let mut themes = crate::ui::presets::builtins();
            for (id, base, background) in [
                ("custom-dark-name", "light", 0xf1ede1),
                ("custom-light-name", "dark", 0x121827),
            ] {
                let mut theme = themes
                    .iter()
                    .find(|theme| theme.id == base)
                    .unwrap()
                    .clone();
                theme.id = id.into();
                theme.name = id.into();
                theme.background = crate::ui::presets::Fill::Vertical {
                    top: background,
                    bottom: background,
                };
                theme.opacity = Some(0.6);
                themes.push(theme);
            }
            cx.set_global(crate::ui::presets::Themes(themes));
            app.set_theme_follow_system(false, window, cx);
            app.set_preset("light", window, cx);
            app.editor_install_file(host, "/theme.rs".into(), original.clone(), None, window, cx);
        });
        let input = app.read_with(&vcx, |app, _| {
            app.tab_code().unwrap().active_file().unwrap().input.clone()
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        vcx.simulate_keystrokes("x");
        vcx.simulate_keystrokes("shift-right shift-right");
        input.update(&mut vcx, |input, cx| {
            input.set_scroll_offset(gpui::point(px(0.), px(-250.)), cx)
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let edited = input.read_with(&vcx, |input, _| input.value());
        let cursor = input.read_with(&vcx, |input, _| input.cursor_position());
        let selected = input.read_with(&vcx, |input, _| input.selected_range());
        let scroll = input.read_with(&vcx, |input, _| input.scroll_offset());
        assert!(!selected.is_empty());
        assert!(scroll.y < px(0.));
        for (legacy, preset, background) in [
            ("auto", "light", 0xffffff),
            ("auto", "dark", 0x000000),
            ("atom_one_dark", "light", 0xffffff),
            ("atom_one_light", "dark", 0x000000),
            ("one_dark_pro", "light", 0xffffff),
            ("one_dark_pro", "one_dark_pro", 0x282c34),
            ("future-theme", "dracula", 0x282a36),
            ("auto", "nord", 0x2e3440),
            ("atom_one_dark", "custom-dark-name", 0xf1ede1),
            ("atom_one_light", "custom-light-name", 0x121827),
        ] {
            app.update_in(&mut vcx, |app, window, cx| {
                let mut saved = serde_json::to_value(&cx.global::<Config>().0).unwrap();
                saved["editor_theme"] = serde_json::json!(legacy);
                cx.set_global(Config(serde_json::from_value(saved).unwrap()));
                app.set_preset(preset, window, cx);
                let style = editor_theme::current(cx);
                assert_eq!(
                    style.highlight_theme.style.editor_background,
                    Some(gpui::rgb(background).into())
                );
                assert_eq!(
                    style.highlight_theme.style.editor_gutter_background,
                    Some(gpui::rgb(background).into())
                );
                assert_eq!(
                    style.highlight_theme.style.editor_background.unwrap().a,
                    1.0
                );
            });
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            input.read_with(&vcx, |input, _| {
                assert_eq!(input.value(), edited);
                assert_eq!(input.cursor_position(), cursor);
                assert_eq!(input.selected_range(), selected);
                assert_eq!(input.scroll_offset(), scroll);
            });
            app.read_with(&vcx, |app, _| {
                assert!(app.tab_code().unwrap().active_file().unwrap().dirty)
            });
        }
        app.update_in(&mut vcx, |app, window, cx| app.focus_editor(window, cx));
        vcx.simulate_keystrokes("secondary-z");
        input.read_with(&vcx, |input, _| assert_eq!(input.value(), original));
    }

    #[gpui::test]
    fn editor_theme_and_typography_are_independent_and_keep_the_buffer(cx: &mut TestAppContext) {
        use crate::core::config::Config;
        use crate::ui::editor_theme;
        let (app, mut vcx) = markdown_window(cx);
        let original = "// comment\nconst root = obj.parent;\n".repeat(60);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/theme.js".into(),
                original.clone(),
                None,
                window,
                cx,
            );
            app.set_editor_font_family("Hack".into(), cx);
            app.set_editor_font_size(16., cx);
            app.set_editor_line_height(1.8, cx);
        });
        let input = app.read_with(&vcx, |app, _| {
            app.tab_code().unwrap().active_file().unwrap().input.clone()
        });
        vcx.run_until_parked();
        vcx.simulate_keystrokes("x shift-right shift-right");
        let edited = input.read_with(&vcx, |input, _| input.value());
        let selected = input.read_with(&vcx, |input, _| input.selected_range());
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_theme_follow_system(false, window, cx);
            app.set_preset("one_dark_pro", window, cx);
            assert_eq!(editor_theme::resolved_name(cx), "Atom One Dark");
            app.preview_preset("light", window, cx);
            assert_eq!(editor_theme::resolved_name(cx), "GitHub Light");
            app.cancel_preset_preview(window, cx);
            assert_eq!(editor_theme::resolved_name(cx), "Atom One Dark");
            app.set_preset("dracula", window, cx);
            assert_eq!(editor_theme::resolved_name(cx), "Atom One Dark");
            app.set_preset("light", window, cx);
            assert_eq!(editor_theme::resolved_name(cx), "GitHub Light");
            assert_eq!(cx.global::<Config>().editor_font_size, 16.);
            assert_eq!(cx.global::<Config>().editor_line_height, 1.8);
            assert_eq!(editor_theme::font_family(cx).as_ref(), "Hack");
            let component_size = cx.theme().mono_font_size;
            let terminal_size = cx.global::<Config>().font_size;
            app.focus_editor(window, cx);
            app.change_focused_font_size(1., window, cx);
            assert_eq!(cx.global::<Config>().editor_font_size, 17.);
            assert_eq!(cx.global::<Config>().font_size, terminal_size);
            assert_eq!(cx.theme().mono_font_size, component_size);
            app.change_ui_font_size(2., cx);
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        input.read_with(&vcx, |input, _| {
            assert_eq!(input.value(), edited);
            assert_eq!(input.selected_range(), selected);
            // GPUI rounds line heights to whole logical pixels.
            assert_eq!(
                input.line_height(),
                Some(px((17.0_f32 * 1.8).round())),
                "source line height uses its font size, independently of UI scale"
            );
        });
        app.read_with(&vcx, |app, _| {
            assert!(app.tab_code().unwrap().active_file().unwrap().dirty)
        });
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_editor_font_family("terminal".into(), cx);
            assert_eq!(
                editor_theme::font_family(cx).as_ref(),
                cx.global::<Config>().font_family
            );
            app.reset_focused_font_size(window, cx);
            assert_eq!(cx.global::<Config>().editor_font_size, 13.);
            app.focus_editor(window, cx);
        });
        vcx.simulate_keystrokes("secondary-z");
        input.read_with(&vcx, |input, _| assert_eq!(input.value(), original));
    }

    #[gpui::test]
    fn editor_theme_tracks_preview_cancel_and_config_reload(cx: &mut TestAppContext) {
        use crate::core::config::{Config, LoadOutcome};
        use crate::ui::editor_theme;
        let (app, mut vcx) = markdown_window(cx);
        vcx.update(|_, cx| crate::ui::windows::WindowRegistry::init(cx));
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_theme_follow_system(false, window, cx);
            app.set_preset("light", window, cx);
            app.preview_preset("dark", window, cx);
            assert_eq!(
                editor_theme::current(cx)
                    .highlight_theme
                    .style
                    .editor_background,
                Some(gpui::rgb(0x000000).into())
            );
            app.cancel_preset_preview(window, cx);
            assert_eq!(
                editor_theme::current(cx)
                    .highlight_theme
                    .style
                    .editor_background,
                Some(gpui::rgb(0xffffff).into())
            );
        });
        vcx.update(|_, cx| {
            for (preset, background) in [("one_dark_pro", 0x282c34), ("light", 0xffffff)] {
                let mut saved = serde_json::to_value(&cx.global::<Config>().0).unwrap();
                saved["editor_theme"] = serde_json::json!("atom_one_dark");
                saved["theme_preset"] = serde_json::json!(preset);
                let config = Config(serde_json::from_value(saved).unwrap());
                crate::apply_reloaded_config(cx, (config, LoadOutcome::Parsed), &mut false);
                assert_eq!(
                    editor_theme::current(cx)
                        .highlight_theme
                        .style
                        .editor_background,
                    Some(gpui::rgb(background).into())
                );
            }
        });
    }

    #[gpui::test]
    fn markdown_opening_keeps_an_anchor_while_its_background_parse_is_pending(
        cx: &mut TestAppContext,
    ) {
        let (app, mut vcx) = markdown_window(cx);
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        let path = PathBuf::from("/markdown-tests/background.md");
        let content = (0..40)
            .map(|n| format!("## Section {n}\n\nParagraph {n}.\n\n"))
            .collect::<String>();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(host.clone(), path.clone(), content, None, window, cx);
            let reading = app
                .tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .clone()
                .unwrap();
            assert!(
                reading.read(cx).is_loading(cx),
                "opening must enqueue the parse"
            );
            app.editor_open_markdown_link(host, &path, Some("section-30".into()), window, cx);
        });
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .clone()
                .unwrap()
        });
        wait_for_markdown(&mut vcx, |cx| {
            reading.read_with(cx, |reading, cx| !reading.is_loading(cx))
        });
        for _ in 0..4 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        reading.read_with(&vcx, |reading, cx| {
            let target = reading.text.read(cx).anchor_bounds("section-30").unwrap();
            assert!((target.top() - reading.scroll.bounds().top() - px(12.)).abs() < px(1.));
            assert!(reading.scroll.offset().y < px(-300.));
        });
        vcx.update(|window, cx| {
            assert!(
                gpui_component::Root::read(window, cx)
                    .notification
                    .read(cx)
                    .notifications()
                    .is_empty(),
                "opening must not report a missing anchor"
            );
        });
    }

    #[gpui::test]
    fn repository_readme_opens_and_draws_in_preview(cx: &mut TestAppContext) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("README.md");
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_open_on_host(host, &path, window, cx)
        });
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code().is_some_and(|code| !code.files.is_empty())
            })
        });
        for _ in 0..3 {
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        app.read_with(&vcx, |app, _| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(file.preview);
            assert!(file.reading.is_some());
        });
    }

    #[gpui::test]
    #[ignore = "manual Markdown opening and switching profile; run with --ignored --nocapture"]
    fn markdown_open_profile(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        for filename in [
            "README.md",
            "docs/examples/markdown-github-theme.md",
            "README.zh-CN.md",
        ] {
            let source = std::fs::read_to_string(filename).unwrap();
            let started = std::time::Instant::now();
            app.update_in(&mut vcx, |app, window, cx| {
                app.editor_install_file(host.clone(), filename.into(), source, None, window, cx)
            });
            let installed = started.elapsed();
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            let response_frame = started.elapsed();
            wait_for_markdown(&mut vcx, |cx| {
                app.read_with(cx, |app, cx| {
                    !app.tab_code()
                        .unwrap()
                        .active_file()
                        .unwrap()
                        .reading
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .is_loading(cx)
                })
            });
            vcx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            eprintln!(
                "Markdown profile {filename}: install={installed:?}, response frame={response_frame:?}, content drawn={:?}",
                started.elapsed()
            );
        }
        let started = std::time::Instant::now();
        for _ in 0..10 {
            for filename in [
                "README.md",
                "docs/examples/markdown-github-theme.md",
                "README.zh-CN.md",
            ] {
                app.update_in(&mut vcx, |app, window, cx| {
                    assert!(app.editor_activate_open(host.id(), Path::new(filename), window, cx))
                });
                vcx.update(|window, cx| {
                    window.refresh();
                    let _ = window.draw(cx);
                });
            }
        }
        eprintln!("Markdown profile 30 warm switches: {:?}", started.elapsed());
    }

    #[gpui::test]
    #[ignore = "manual fixed-document frame benchmark; run with --ignored --nocapture"]
    fn markdown_large_document_frame_profile(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        vcx.simulate_resize(gpui::size(px(1200.), px(850.)));
        let source = include_str!("../../tests/fixtures/markdown/large-navigation.md");
        let start = std::time::Instant::now();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/large-navigation.md".into(),
                source.into(),
                None,
                window,
                cx,
            );
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let response_ms = start.elapsed().as_secs_f64() * 1000.;
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .as_ref()
                .unwrap()
                .clone()
        });
        wait_for_markdown(&mut vcx, |cx| {
            reading.read_with(cx, |view, cx| {
                !view.is_loading(cx) && !view.pending_diagrams_for_profile()
            })
        });
        let ready_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut samples = Vec::new();
        let mut min_offset = px(0.);
        for frame in 0..110 {
            let start = std::time::Instant::now();
            vcx.simulate_event(gpui::ScrollWheelEvent {
                position: gpui::point(px(1100.), px(500.)),
                delta: gpui::ScrollDelta::Pixels(gpui::point(
                    px(0.),
                    px(if frame < 55 { -120. } else { 120. }),
                )),
                modifiers: gpui::Modifiers::none(),
                touch_phase: gpui::TouchPhase::Moved,
            });
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
                reading.read(cx).verify_profile_scroll_frame(cx);
            });
            min_offset = min_offset
                .min(reading.read_with(&vcx, |view, cx| view.profile_scroll_offset(cx).y));
            if frame >= 10 {
                samples.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        samples.sort_by(f64::total_cmp);
        assert!(
            min_offset < px(-500.),
            "the benchmark must navigate the actual reading pane"
        );
        eprintln!(
            "MARKDOWN_FRAME_PROFILE {}",
            serde_json::json!({
                "source_bytes": source.len(), "window": [1200, 850], "samples": samples.len(),
                "first_response_ms": response_ms, "all_diagrams_ready_ms": ready_ms,
                "scroll_cpu_frame_median_ms": samples[samples.len()/2],
                "scroll_cpu_frame_p95_ms": samples[samples.len()*95/100],
                "scroll_cpu_frame_max_ms": samples[samples.len()-1],
            })
        );
    }

    #[gpui::test]
    fn large_markdown_virtualizes_offscreen_blocks_and_keeps_end_navigation(
        cx: &mut TestAppContext,
    ) {
        let (app, mut vcx) = markdown_window(cx);
        vcx.simulate_resize(gpui::size(px(1200.), px(850.)));
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/large-navigation.md".into(),
                include_str!("../../tests/fixtures/markdown/large-navigation.md").into(),
                None,
                window,
                cx,
            );
        });
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .as_ref()
                .unwrap()
                .clone()
        });
        wait_for_markdown(&mut vcx, |cx| {
            reading.read_with(cx, |view, cx| {
                !view.is_loading(cx) && !view.pending_diagrams_for_profile()
            })
        });
        assert!(
            reading.read_with(&vcx, |view, cx| view
                .text
                .read(cx)
                .anchor_bounds("document-end")
                .is_none()),
            "the final heading must not be prepainted while the viewport is at the beginning"
        );
        vcx.update(|window, cx| window.focus(&reading.read(cx).focus_handle(cx), cx));
        vcx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-down"
        } else {
            "ctrl-end"
        });
        for _ in 0..3 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        reading.read_with(&vcx, |view, cx| {
            let target = view
                .text
                .read(cx)
                .anchor_bounds("document-end")
                .expect("End must lay out the final heading");
            assert!(
                target.intersects(&view.scroll.bounds()),
                "End must reveal the final document section"
            );
        });
        reading.update(&mut vcx, |view, cx| {
            view.navigate_anchor("wide-flowchart".into(), cx)
        });
        for _ in 0..3 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        reading.read_with(&vcx, |view, cx| {
            let target = view.text.read(cx).anchor_bounds("wide-flowchart").unwrap();
            assert!(
                target.intersects(&view.scroll.bounds()),
                "links must reach an offscreen heading"
            );
        });
    }

    #[gpui::test]
    fn markdown_link_click_survives_a_frame_between_press_and_release(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let content = format!(
            "# Links\n\n[Jump](#target)\n\n{}> # Target\n\n{}",
            "> Paragraph in a long blockquote.\n>\n".repeat(80),
            "Paragraph after the target.\n\n".repeat(1400)
        );
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/links.md".into(),
                content,
                None,
                window,
                cx,
            );
        });
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .as_ref()
                .unwrap()
                .clone()
        });
        wait_for_markdown(&mut vcx, |cx| {
            reading.read_with(cx, |view, cx| !view.is_loading(cx))
        });
        for _ in 0..3 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        let bounds =
            reading.read_with(&vcx, |view, cx| view.text.read(cx).block_bounds(1).unwrap());
        let position = gpui::point(bounds.left() + px(20.), bounds.center().y);
        vcx.simulate_event(gpui::MouseDownEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        vcx.simulate_event(gpui::MouseUpEvent {
            position,
            button: gpui::MouseButton::Left,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
        });
        for _ in 0..3 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        assert!(
            reading.read_with(&vcx, |view, cx| view
                .text
                .read(cx)
                .anchor_bounds("target")
                .is_some_and(|target| target.intersects(&view.profile_scroll_bounds(cx)))),
            "a real click must reveal a deeply nested heading after a repaint"
        );
    }

    #[gpui::test]
    fn markdown_opens_in_preview_and_preserves_edit_undo_save_and_focus(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.MARKDOWN");
        let original = "# Reading\n\nThe source buffer owns this text.\n";
        std::fs::write(&path, original).unwrap();
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        let (app, mut vcx) = markdown_window(cx);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_open_on_host(host, &path, window, cx)
        });
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code().is_some_and(|code| !code.files.is_empty())
            })
        });
        let (input, reading) = app.update_in(&mut vcx, |app, window, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(file.preview);
            assert!(!file.dirty);
            assert!(
                file.reading
                    .as_ref()
                    .unwrap()
                    .focus_handle(cx)
                    .is_focused(window)
            );
            assert!(!file.input.focus_handle(cx).is_focused(window));
            (file.input.clone(), file.reading.clone().unwrap())
        });
        vcx.simulate_keystrokes("x backspace secondary-v secondary-z");
        assert_eq!(
            input.read_with(&vcx, |input, _| input.text().to_string()),
            original
        );
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_toggle_preview(window, cx)
        });
        vcx.simulate_keystrokes("x");
        let edited = input.read_with(&vcx, |input, _| input.text().to_string());
        assert_ne!(edited, original);
        let cursor = input.read_with(&vcx, |input, _| input.cursor_position());
        for _ in 0..2 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let control = vcx
                .debug_bounds("document-fill-toggle")
                .expect("the editor offers fill and restore");
            vcx.simulate_click(control.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            app.read_with(&vcx, |app, _| {
                let file = app.tab_code().unwrap().active_file().unwrap();
                assert_eq!(file.input.entity_id(), input.entity_id());
                assert!(file.dirty, "layout switches keep unsaved edits");
            });
            assert_eq!(
                input.read_with(&vcx, |input, _| input.text().to_string()),
                edited
            );
            assert_eq!(
                input.read_with(&vcx, |input, _| input.cursor_position()),
                cursor
            );
        }
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_toggle_preview(window, cx)
        });
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        reading.read_with(&vcx, |preview, cx| {
            assert_eq!(preview.text.read(cx).source().as_ref(), edited)
        });
        app.read_with(&vcx, |app, _| {
            assert!(app.tab_code().unwrap().active_file().unwrap().dirty)
        });
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            original,
            "mode switches must not save"
        );
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_toggle_preview(window, cx)
        });
        assert_eq!(
            input.read_with(&vcx, |input, _| input.cursor_position()),
            cursor
        );
        vcx.simulate_keystrokes("secondary-z");
        assert_eq!(
            input.read_with(&vcx, |input, _| input.text().to_string()),
            original,
            "undo must survive the preview round trip"
        );
        vcx.simulate_keystrokes("y");
        let saved = input.read_with(&vcx, |input, _| input.text().to_string());
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_toggle_preview(window, cx)
        });
        vcx.simulate_keystrokes("secondary-s");
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code()
                    .unwrap()
                    .active_file()
                    .is_some_and(|file| !file.dirty && file.saving.is_none())
            })
        });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
        app.update_in(&mut vcx, |app, window, cx| {
            assert!(app.document_close_active_if_focused(window, cx));
            assert!(app.tab_code().unwrap().files.is_empty());
        });
    }

    #[gpui::test]
    fn markdown_reactivation_keeps_mode_but_explicit_line_navigation_opens_source(
        cx: &mut TestAppContext,
    ) {
        let (app, mut vcx) = markdown_window(cx);
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        let path = PathBuf::from("/markdown-tests/first.md");
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                host.clone(),
                path.clone(),
                "# First\n\nText\n".into(),
                None,
                window,
                cx,
            );
            app.editor_toggle_preview(window, cx);
            app.editor_install_file(
                host.clone(),
                "/markdown-tests/source.rs".into(),
                "fn main() {}".into(),
                None,
                window,
                cx,
            );
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
            assert!(app.editor_activate_open(host.id(), &path, window, cx));
            assert!(!app.tab_code().unwrap().active_file().unwrap().preview);
            app.editor_toggle_preview(window, cx);
            app.open_file_in_editor_at(&path, Some(3), Some(2), window, cx);
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(!file.preview);
            assert!(file.input.focus_handle(cx).is_focused(window));
            app.editor_close_file(0, window, cx);
            app.editor_install_file(host, path, "# Reopened".into(), None, window, cx);
            assert!(app.tab_code().unwrap().active_file().unwrap().preview);
        });
    }

    #[gpui::test]
    fn markdown_failed_anchor_does_not_override_later_source_navigation(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guide.md");
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_open_markdown_link(host, &path, Some("intro".into()), window, cx);
        });
        wait_for_markdown(&mut vcx, |cx| {
            cx.update(|window, cx| {
                !gpui_component::Root::read(window, cx)
                    .notification
                    .read(cx)
                    .notifications()
                    .is_empty()
            })
        });
        std::fs::write(&path, "# Intro\n\nTarget line\n").unwrap();
        app.update_in(&mut vcx, |app, window, cx| {
            app.open_file_in_editor_at(&path, Some(3), Some(2), window, cx);
        });
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code().is_some_and(|code| !code.files.is_empty())
            })
        });
        app.update_in(&mut vcx, |app, window, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(!file.preview);
            assert!(file.input.focus_handle(cx).is_focused(window));
            assert_eq!(
                file.input.read(cx).cursor_position(),
                Position {
                    line: 2,
                    character: 1
                }
            );
        });
    }

    #[gpui::test]
    fn markdown_newer_source_request_supersedes_an_inflight_anchor(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guide.md");
        std::fs::write(&path, "# Intro\n\nTarget line\n").unwrap();
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        // Both requests start before the UI executor can deliver either Host
        // result. The last click must choose source, regardless of read order.
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_open_markdown_link(host, &path, Some("intro".into()), window, cx);
            app.open_file_in_editor_at(&path, Some(3), Some(2), window, cx);
        });
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code().is_some_and(|code| !code.files.is_empty())
            })
        });
        app.update_in(&mut vcx, |app, window, cx| {
            let code = app.tab_code().unwrap();
            assert_eq!(code.files.len(), 1);
            let file = code.active_file().unwrap();
            assert!(!file.preview);
            assert!(file.input.focus_handle(cx).is_focused(window));
            assert_eq!(
                file.input.read(cx).cursor_position(),
                Position {
                    line: 2,
                    character: 1
                }
            );
        });
    }

    #[gpui::test]
    fn markdown_platform_keys_scroll_without_changing_source(cx: &mut TestAppContext) {
        let (app, mut vcx) = markdown_window(cx);
        let content = "# Heading\n\nParagraph.\n\n".repeat(80);
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/markdown-tests/keys.md".into(),
                content.clone(),
                None,
                window,
                cx,
            );
        });
        for _ in 0..3 {
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
        }
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .clone()
                .unwrap()
        });
        let (end, home) = if cfg!(target_os = "macos") {
            ("cmd-down", "cmd-up")
        } else {
            ("ctrl-end", "ctrl-home")
        };
        vcx.simulate_keystrokes(end);
        reading.read_with(&vcx, |reading, _| {
            assert!(reading.scroll.max_offset().y > px(0.));
            assert_eq!(reading.scroll.offset().y, -reading.scroll.max_offset().y);
        });
        vcx.simulate_keystrokes(home);
        assert_eq!(
            reading.read_with(&vcx, |reading, _| reading.scroll.offset().y),
            px(0.)
        );
        vcx.simulate_keystrokes("pagedown");
        assert!(reading.read_with(&vcx, |reading, _| reading.scroll.offset().y) < px(0.));
        let reading_offset = reading.read_with(&vcx, |reading, _| reading.scroll.offset());
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_install_file(
                tty7_core::host::local::LocalHost::new(),
                "/markdown-tests/other.rs".into(),
                "fn main() {}".into(),
                None,
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        let tab = vcx.debug_bounds("editor-file-tab-0").unwrap();
        vcx.simulate_click(tab.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        assert_eq!(
            reading.read_with(&vcx, |reading, _| reading.scroll.offset()),
            reading_offset,
            "switching file tabs preserves the reading position"
        );
        vcx.simulate_keystrokes("secondary-a secondary-c");
        reading.read_with(&vcx, |reading, cx| {
            assert!(reading.text.read(cx).selected_text().contains("Paragraph."));
        });
        app.read_with(&vcx, |app, cx| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert!(!file.dirty);
            assert_eq!(file.input.read(cx).text().to_string(), content);
        });
    }

    #[gpui::test]
    fn markdown_theme_reload_preserves_document_selection_and_scroll(cx: &mut TestAppContext) {
        use crate::core::{config::Config, markdown_theme};
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let theme_path = dir.path().join("study.yaml");
        let yaml = "schema_version: 2\nid: study\nname: Study\nlight: {link: '#123456'}\ndark: {link: '#654321'}\n";
        std::fs::write(&theme_path, yaml).unwrap();
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        let content = (0..80)
            .map(|n| format!("## Section {n}\n\nReading paragraph {n}.\n\n"))
            .collect::<String>();
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_theme_follow_system(false, window, cx);
            app.set_preset("light", window, cx);
            app.set_markdown_theme("github", cx);
            crate::ui::markdown_preview::apply_snapshot(markdown_theme::scan(Some(dir.path())), cx);
            app.editor_install_file(
                host,
                "/markdown-tests/theme.md".into(),
                content.clone(),
                None,
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .clone()
                .unwrap()
        });
        let text = reading.read_with(&vcx, |reading, _| reading.text.clone());
        // The first frame measures the viewport; the next applies its compact
        // or wide layout. Establish selection only after that resize, as a
        // user does in an already visible document.
        for _ in 0..2 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        text.update(&mut vcx, |text, cx| text.select_all(cx));
        reading.update(&mut vcx, |reading, cx| {
            reading.scroll.set_offset(gpui::point(px(0.), px(-300.)));
            cx.notify();
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let selected = text.read_with(&vcx, |text, cx| {
            assert_eq!(
                cx.global::<gpui_component::Theme>().mode,
                gpui_component::ThemeMode::Light
            );
            let selected = text.selected_text();
            assert!(selected.contains("Section 79"));
            selected
        });
        let before = reading.read_with(&vcx, |reading, _| reading.scroll.offset());
        assert!(before.y < px(0.));
        for _ in 0..2 {
            let control = vcx
                .debug_bounds("document-fill-toggle")
                .expect("Markdown offers fill and restore");
            vcx.simulate_click(control.center(), gpui::Modifiers::none());
            vcx.run_until_parked();
            text.read_with(&vcx, |text, _| {
                assert_eq!(text.source().as_ref(), content);
                assert_eq!(text.selected_text(), selected);
            });
            app.read_with(&vcx, |app, _| {
                assert_eq!(
                    app.tab_code()
                        .unwrap()
                        .active_file()
                        .unwrap()
                        .reading
                        .as_ref()
                        .unwrap()
                        .entity_id(),
                    reading.entity_id()
                );
            });
        }
        assert_eq!(
            reading.read_with(&vcx, |reading, _| reading.scroll.offset()),
            before
        );
        app.update_in(&mut vcx, |app, window, cx| {
            app.set_markdown_theme("study", cx);
            app.set_preset("dark", window, cx);
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert_eq!(
            reading.read_with(&vcx, |reading, _| reading.scroll.offset()),
            before
        );
        text.read_with(&vcx, |text, cx| {
            assert_eq!(
                cx.global::<gpui_component::Theme>().mode,
                gpui_component::ThemeMode::Dark
            );
            assert_eq!(text.source().as_ref(), content);
            assert_eq!(text.selected_text(), selected);
        });
        std::fs::write(&theme_path, "invalid: [").unwrap();
        vcx.update(|_, cx| {
            crate::ui::markdown_preview::apply_snapshot(markdown_theme::scan(Some(dir.path())), cx);
            assert_eq!(cx.global::<Config>().markdown_theme, "study");
            assert_eq!(crate::ui::markdown_preview::current(cx).theme.id, "study");
        });
        std::fs::write(&theme_path, yaml.replace("#654321", "#abcdef")).unwrap();
        vcx.update(|_, cx| {
            crate::ui::markdown_preview::apply_snapshot(markdown_theme::scan(Some(dir.path())), cx)
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert_eq!(
            reading.read_with(&vcx, |reading, _| reading.scroll.offset()),
            before
        );
        text.read_with(&vcx, |text, _| {
            assert_eq!(text.source().as_ref(), content);
            assert_eq!(text.selected_text(), selected);
        });
        app.read_with(&vcx, |app, _| {
            let file = app.tab_code().unwrap().active_file().unwrap();
            assert_eq!(
                file.reading.as_ref().unwrap().entity_id(),
                reading.entity_id()
            );
            assert!(file.preview);
            assert!(!file.dirty);
        });
    }

    #[gpui::test]
    fn markdown_document_links_scroll_to_the_requested_heading_and_keep_reading_position(
        cx: &mut TestAppContext,
    ) {
        let (app, mut vcx) = markdown_window(cx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guide.md");
        let content = (0..80)
            .map(|n| format!("## Section {n}\n\nReading paragraph {n}.\n\n"))
            .collect::<String>();
        std::fs::write(&path, &content).unwrap();
        let host: SharedHost = tty7_core::host::local::LocalHost::new();
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_open_markdown_link(
                host.clone(),
                &path,
                Some("section-30".into()),
                window,
                cx,
            )
        });
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code().is_some_and(|code| !code.files.is_empty())
            })
        });
        for _ in 0..4 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        let reading = app.read_with(&vcx, |app, _| {
            app.tab_code()
                .unwrap()
                .active_file()
                .unwrap()
                .reading
                .clone()
                .unwrap()
        });
        reading.read_with(&vcx, |reading, cx| {
            let target = reading.text.read(cx).anchor_bounds("section-30").unwrap();
            assert!(
                (target.top() - reading.scroll.bounds().top() - px(12.)).abs() < px(1.),
                "anchor should be at the top of the reading viewport: {target:?}, viewport {:?}, offset {:?}", reading.scroll.bounds(), reading.scroll.offset()
            );
            assert!(reading.scroll.offset().y < px(-300.));
        });

        // A package that changes typography should retain the visible paragraph,
        // even though its absolute pixel offset changes after layout.
        let position = reading.read_with(&vcx, |reading, cx| {
            reading
                .text
                .read(cx)
                .reading_position(reading.scroll.bounds().top())
                .unwrap()
        });
        std::fs::write(dir.path().join("large.yaml"), "schema_version: 2\nid: large\nname: Large\ntypography: {font_size: 22, heading_sizes: [42, 36, 30, 27, 24, 22]}\nlight: {}\ndark: {}\n").unwrap();
        app.update_in(&mut vcx, |app, _, cx| {
            crate::ui::markdown_preview::apply_snapshot(
                crate::core::markdown_theme::scan(Some(dir.path())),
                cx,
            );
            app.set_markdown_theme("large", cx);
        });
        for _ in 0..4 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        let updated = reading.read_with(&vcx, |reading, cx| {
            let target = reading.text.read(cx).block_bounds(position.0).unwrap();
            reading.scroll.bounds().top() - target.top()
        });
        // The previous block can grow into the gap above this anchor after
        // reflow. Verify the saved content anchor rather than reclassifying
        // whichever block now touches the viewport's first pixel.
        assert!((updated - position.1).abs() < px(1.));

        // A fragment can also bring an already-open source editor back to its
        // existing preview, without creating a second source buffer.
        app.update_in(&mut vcx, |app, window, cx| {
            app.editor_toggle_preview(window, cx);
            app.editor_open_markdown_link(host, &path, Some("section-10".into()), window, cx);
        });
        wait_for_markdown(&mut vcx, |cx| {
            app.read_with(cx, |app, _| {
                app.tab_code().unwrap().active_file().unwrap().preview
            })
        });
        for _ in 0..4 {
            vcx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        app.read_with(&vcx, |app, _| {
            assert_eq!(app.tab_code().unwrap().files.len(), 1)
        });
        reading.read_with(&vcx, |reading, cx| {
            let target = reading.text.read(cx).anchor_bounds("section-10").unwrap();
            assert!((target.top() - reading.scroll.bounds().top() - px(12.)).abs() < px(1.));
        });
    }

    #[test]
    fn language_map_covers_common_extensions() {
        for (path, lang) in [
            ("a/b/main.rs", "rust"),
            ("x.tsx", "tsx"),
            ("x.jsx", "javascript"),
            ("x.yml", "yaml"),
            ("Makefile", "make"),
            ("CMakeLists.txt", "cmake"),
            (".zshrc", "bash"),
            ("notes.md", "markdown"),
            ("notes.MD", "markdown"),
            ("notes.markdown", "markdown"),
            ("notes.MARKDOWN", "markdown"),
            ("query.SQL", "sql"),
            ("unknown.xyz", "text"),
            ("no_ext", "text"),
        ] {
            assert_eq!(language_for_path(Path::new(path)), lang, "path {path}");
        }
    }

    #[test]
    fn binary_sniff_flags_nul_bytes_only() {
        assert!(looks_binary(b"\x7fELF\x00\x01"));
        assert!(!looks_binary("plain text\nwith lines".as_bytes()));
        assert!(!looks_binary("中文 UTF-8 内容".as_bytes()));
    }

    fn t(secs: i64, nanos: u32) -> Option<MTime> {
        Some(MTime { secs, nanos })
    }

    #[test]
    fn external_changes_are_told_apart_from_our_own_saves() {
        let ours = t(100, 0);

        assert_eq!(
            classify_external_change(false, false, ours, ours),
            ExternalChange::Ignore
        );

        assert_eq!(
            classify_external_change(false, false, ours, t(101, 0)),
            ExternalChange::Reload
        );

        assert_eq!(
            classify_external_change(false, true, ours, t(101, 0)),
            ExternalChange::Conflict
        );

        assert_eq!(
            classify_external_change(false, false, t(100, 0), t(100, 1)),
            ExternalChange::Reload
        );

        assert_eq!(
            classify_external_change(true, false, ours, t(101, 0)),
            ExternalChange::Ignore
        );

        assert_eq!(
            classify_external_change(false, false, None, None),
            ExternalChange::Reload
        );
    }

    #[test]
    fn a_landed_save_only_cleans_a_buffer_that_did_not_move() {
        assert_eq!(
            settle_save(true, 7, 7, false),
            SaveLanding {
                clean: true,
                requeue: false
            }
        );

        assert_eq!(
            settle_save(true, 7, 9, false),
            SaveLanding {
                clean: false,
                requeue: false
            }
        );

        assert_eq!(
            settle_save(true, 7, 9, true),
            SaveLanding {
                clean: false,
                requeue: true
            }
        );

        assert_eq!(
            settle_save(false, 7, 7, true),
            SaveLanding {
                clean: false,
                requeue: false
            }
        );
    }
}
