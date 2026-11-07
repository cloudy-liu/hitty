use super::*;
use crate::terminal::git_status::{GitStatusCache, RepoSnapshot};
use crate::ui::app::{Tab, test_window};
use crate::ui::diff_overlay::DiffLoad;
use crate::ui::host_ops::{Entry, Host, HostId, Meta, Output, SearchHit, SharedHost, WatchSub};
use crate::ui::host_registry::HostRegistry;
use crate::ui::pane::{Pane, PaneSlot};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tty7_core::host::ShellInventory;

// Substitute only the Host I/O boundary, as the document-resource tests do.
// Pane selection, cache, sidebar mouse input, Git parser and diff renderer run
// through their production paths. The real SSH exchange is tested in core.
struct RemoteRepository {
    id: HostId,
    branch: &'static str,
    body: Option<Arc<Mutex<String>>>,
    status_queries: Option<Arc<AtomicUsize>>,
    connection: Option<SharedHost>,
    pause: Option<Arc<QueryPause>>,
}

#[derive(Default)]
struct QueryPause {
    armed: AtomicBool,
    started: AtomicUsize,
    released: Mutex<bool>,
    wake: Condvar,
}

impl QueryPause {
    fn wait(&self) {
        if !self.armed.load(Ordering::SeqCst) {
            return;
        }
        self.started.fetch_add(1, Ordering::SeqCst);
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.wake.wait(released).unwrap();
        }
    }
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.wake.notify_all();
    }
}

struct ReleasePause(Arc<QueryPause>);
impl Drop for ReleasePause {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn unsupported<T>() -> io::Result<T> {
    Err(io::ErrorKind::Unsupported.into())
}

impl Host for RemoteRepository {
    fn id(&self) -> HostId {
        self.id
    }
    fn separator(&self) -> char {
        '/'
    }
    fn is_absolute(&self, p: &Path) -> bool {
        p.to_string_lossy().starts_with('/')
    }
    fn read_dir(&self, _: &Path, _: Option<&Path>) -> io::Result<Vec<Entry>> {
        unsupported()
    }
    fn stat(&self, _: &Path) -> io::Result<Meta> {
        unsupported()
    }
    fn read_file(&self, p: &Path, _: u64) -> io::Result<Vec<u8>> {
        if !self.is_connected() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        if let Some(pause) = &self.pause {
            pause.wait();
        }
        if p.to_string_lossy() == "/work/project/docs/new.md" {
            Ok(format!("{} untracked\n", self.branch).into_bytes())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
    fn canonicalize(&self, _: &Path) -> io::Result<PathBuf> {
        unsupported()
    }
    fn search(
        &self,
        _: &[PathBuf],
        _: &str,
        _: usize,
        _: usize,
        _: bool,
    ) -> io::Result<Vec<SearchHit>> {
        unsupported()
    }
    fn write_file(&self, _: &Path, _: &[u8]) -> io::Result<Meta> {
        unsupported()
    }
    fn create_file_new(&self, _: &Path) -> io::Result<()> {
        unsupported()
    }
    fn create_dir(&self, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    fn rename(&self, _: &Path, _: &Path) -> io::Result<()> {
        unsupported()
    }
    fn remove(&self, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    fn repo_root(&self, _: &Path) -> io::Result<Option<PathBuf>> {
        Ok(Some("/work/project".into()))
    }
    fn git(&self, cwd: &Path, args: &[&str]) -> io::Result<Output> {
        if !self.is_connected() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        if let Some(pause) = &self.pause {
            pause.wait();
        }
        if cwd != Path::new("/work/project") {
            return Err(io::ErrorKind::NotFound.into());
        }
        let stdout = if args.first() == Some(&"rev-parse") {
            if args.contains(&"--git-dir") {
                "/work/project\n/work/project/.git\n/work/project/.git\n".to_string()
            } else {
                "/work/project\n".to_string()
            }
        } else if args.first() == Some(&"symbolic-ref") {
            format!("{}\n", self.branch)
        } else if args.contains(&"--numstat") {
            if let Some(queries) = &self.status_queries {
                queries.fetch_add(1, Ordering::SeqCst);
            }
            "2\t1\tsrc/service.rs\n".to_string()
        } else if args.contains(&"ls-files") {
            "docs/new.md\n".to_string()
        } else if args.first() == Some(&"diff") || args.contains(&"diff") {
            format!(
                "diff --git a/src/service.rs b/src/service.rs\n--- a/src/service.rs\n+++ b/src/service.rs\n@@ -1 +1,2 @@\n-old\n+{}\n+added\n",
                self.body
                    .as_ref()
                    .map(|body| body.lock().unwrap().clone())
                    .unwrap_or_else(|| self.branch.into())
            )
        } else {
            return unsupported();
        };
        Ok(Output {
            status: Some(0),
            stdout: stdout.into_bytes(),
            stderr: vec![],
        })
    }
    fn is_connected(&self) -> bool {
        self.connection
            .as_ref()
            .is_none_or(|host| host.is_connected())
    }
    fn shells(&self) -> io::Result<ShellInventory> {
        unsupported()
    }
    fn watch(&self, _: &[PathBuf]) -> io::Result<WatchSub> {
        unsupported()
    }
}

fn settle(vcx: &mut VisualTestContext, mut ready: impl FnMut(&mut VisualTestContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        vcx.run_until_parked();
        if ready(vcx) {
            return;
        }
        assert!(Instant::now() < deadline, "remote diff did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[gpui::test]
fn sidebar_click_reads_the_owning_ssh_repository_and_untracked_file(cx: &mut TestAppContext) {
    let (app, mut vcx) = test_window::harness(cx);
    vcx.simulate_resize(gpui::size(px(1200.), px(700.)));
    let cwd = PathBuf::from("/work/project");
    let (panes, _streams, _hosts): (Vec<Entity<TerminalView>>, Vec<_>, Vec<_>) = app.update_in(&mut vcx, |app, window, cx| {
        let cfg = cx.global_mut::<Config>();
        cfg.tab_bar_position = crate::core::config::TabBarPosition::Left;
        cfg.sidebar_grouping = crate::core::config::SidebarGrouping::None;
        cfg.cursor_blink = false;
        cfg.sidebar_diff_preview = true;
        let mut panes = Vec::new();
        let mut streams = Vec::new();
        let mut hosts = Vec::new();
        for (index, branch) in ["remote-a", "remote-b"].into_iter().enumerate() {
            let pane_id = index as u64 + 70;
            let (parts, mut daemon) = quiet_test_shell_parts(pane_id);
            let view = cx.new(|cx| TerminalView::from_native_ssh_parts(NativeSshParts {
                terminal: parts.terminal, pane_id, restored: false,
                persist: Box::new(serde_json::from_str(r#"{"host":"build-box","port":22,"user":"me","auth_mode":"auto"}"#).unwrap()),
            }, window, cx));
            crate::daemon::protocol::DaemonMsg::RemoteContext(Some(RemoteContext {
                kind: crate::daemon::protocol::RemoteKind::NativeSsh,
                argv: vec![], target: branch.into(),
            })).encode(&mut daemon).unwrap();
            crate::daemon::protocol::DaemonMsg::Cwd(cwd.clone()).encode(&mut daemon).unwrap();
            let id = view.read(cx).git_host_id();
            let connection = HostRegistry::lookup(cx, id).expect("native pane route exists");
            let host: SharedHost = Arc::new(RemoteRepository { id, branch, body: None, status_queries: None, connection: Some(connection), pause: None });
            HostRegistry::insert_pane(cx, &host);
            assert!(!HostRegistry::ids(cx).contains(&id));
            hosts.push(host);
            cx.default_global::<GitStatusCache>().finish_probe(id, &cwd, Some(RepoSnapshot {
                root: cwd.clone(), home: cwd.clone(), branch: branch.into(), counts: Some((2, 1)),
            }));
            app.tabs.push(Tab::new(Pane::leaf(PaneSlot::Ready(view.clone()))));
            panes.push(view);
            streams.push(daemon);
        }
        // An identical local path must not determine what either SSH row opens.
        cx.default_global::<GitStatusCache>().finish_probe(HostId::LOCAL, &cwd, Some(RepoSnapshot {
            root: cwd.clone(), home: cwd.clone(), branch: "local-only".into(), counts: Some((99, 99)),
        }));
        app.active = 0;
        cx.notify();
        (panes, streams, hosts)
    });
    settle(&mut vcx, |vcx| {
        app.update_in(vcx, |_, window, cx| {
            for pane in &panes {
                pane.update(cx, |view, cx| view.poll_foreground(window, cx));
            }
            panes
                .iter()
                .all(|pane| pane.read(cx).git_status_cwd() == Some(cwd.as_path()))
        })
    });
    vcx.run_until_parked();
    let counts = vcx
        .debug_bounds("sidebar-diff-1")
        .expect("remote totals are clickable");
    vcx.simulate_click(counts.center(), gpui::Modifiers::none());
    let remote_id = panes[1].read_with(&vcx, |view, _| view.git_host_id());
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.active, 1, "clicking totals activates their owning tab");
        let overlay = app.tabs[1].diff_overlay.as_ref().expect("diff opened");
        assert_eq!(
            overlay.host_id, remote_id,
            "remote cwd must travel with its pane's Git identity"
        );
        assert_eq!(overlay.cwd, cwd);
    });
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| {
            matches!(
                app.tabs[1].diff_overlay.as_ref().unwrap().load,
                DiffLoad::Ready(_)
            )
        })
    });
    app.read_with(&vcx, |app, _| {
        let DiffLoad::Ready(snapshot) = &app.tabs[1].diff_overlay.as_ref().unwrap().load else {
            unreachable!()
        };
        assert_eq!(snapshot.branch, "remote-b");
        assert_eq!(snapshot.totals(), (2, 1));
        assert_eq!(snapshot.files[0].path, "src/service.rs");
        assert_eq!(snapshot.files[0].hunks[0].lines[1].text, "remote-b");
    });
    app.update_in(&mut vcx, |app, window, cx| {
        assert!(
            !app.tab_is_in_repo(1, window, cx),
            "read-only pane Git must not enable workspace-host write actions"
        );
    });
    // Open the listed untracked file through the same application entry. Its
    // bytes must use the Host's POSIX join even when the GUI runs on Windows.
    app.update_in(&mut vcx, |app, window, cx| {
        app.toggle_diff_overlay_at(
            remote_id,
            cwd.clone(),
            Some("docs/new.md".into()),
            window,
            cx,
        )
    });
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| {
            app.tabs[1].diff_overlay.as_ref().unwrap().preview.is_some()
        })
    });
    app.read_with(&vcx, |app, _| {
        let overlay = app.tabs[1].diff_overlay.as_ref().unwrap();
        let (_, file) = overlay.preview.as_ref().unwrap();
        let file = file.as_ref().expect("the remote untracked file was read");
        assert_eq!(file.hunks[0].lines[0].text, "remote-b untracked");
    });
    app.update_in(&mut vcx, |app, window, cx| {
        app.close_diff_overlay(window, cx);
        panes[1].update(cx, |view, cx| view.release_native_git(cx));
        app.toggle_diff_overlay(remote_id, cwd, window, cx);
        let overlay = app.tabs[1].diff_overlay.as_ref().unwrap();
        assert!(!overlay.loading);
        assert!(
            matches!(overlay.load, DiffLoad::Unavailable),
            "a departed pane reports an unavailable diff instead of reading forever"
        );
    });
}

#[gpui::test]
fn active_ssh_diff_refreshes_external_edits_with_unchanged_line_counts(cx: &mut TestAppContext) {
    let (app, mut vcx) = test_window::harness(cx);
    vcx.simulate_resize(gpui::size(px(1200.), px(700.)));
    let cwd = PathBuf::from("/work/project");
    let body = Arc::new(Mutex::new("before external edit".to_string()));
    let status_queries = Arc::new(AtomicUsize::new(0));
    let pause = Arc::new(QueryPause::default());
    let (pane, daemons, _host) = app.update_in(&mut vcx, |app, window, cx| {
        window.activate_window();
        let cfg = cx.global_mut::<Config>();
        cfg.tab_bar_position = crate::core::config::TabBarPosition::Left;
        cfg.sidebar_grouping = crate::core::config::SidebarGrouping::None;
        cfg.cursor_blink = false;
        cfg.sidebar_diff_preview = true;
        let (parts, mut daemon) = quiet_test_shell_parts(80);
        let pane = cx.new(|cx| {
            TerminalView::from_native_ssh_parts(
                NativeSshParts {
                    terminal: parts.terminal,
                    pane_id: 80,
                    restored: false,
                    persist: Box::new(
                        serde_json::from_str(
                            r#"{"host":"build-box","port":22,"user":"me","auth_mode":"auto"}"#,
                        )
                        .unwrap(),
                    ),
                },
                window,
                cx,
            )
        });
        crate::daemon::protocol::DaemonMsg::RemoteContext(Some(RemoteContext {
            kind: crate::daemon::protocol::RemoteKind::NativeSsh,
            argv: vec![],
            target: "build-box".into(),
        }))
        .encode(&mut daemon)
        .unwrap();
        crate::daemon::protocol::DaemonMsg::Cwd(cwd.clone())
            .encode(&mut daemon)
            .unwrap();
        let id = pane.read(cx).git_host_id();
        let connection = HostRegistry::lookup(cx, id).expect("native pane route exists");
        let host: SharedHost = Arc::new(RemoteRepository {
            id,
            branch: "remote",
            body: Some(body.clone()),
            status_queries: Some(status_queries.clone()),
            connection: Some(connection),
            pause: Some(pause.clone()),
        });
        HostRegistry::insert_pane(cx, &host);
        assert!(!HostRegistry::ids(cx).contains(&id));
        app.tabs
            .push(Tab::new(Pane::leaf(PaneSlot::Ready(pane.clone()))));
        let (local, local_daemon) = quiet_test_pane(81, window, cx);
        app.tabs.push(Tab::new(Pane::leaf(PaneSlot::Ready(local))));
        app.active = 0;
        cx.notify();
        (pane, (daemon, local_daemon), host)
    });
    settle(&mut vcx, |vcx| {
        app.update_in(vcx, |_, window, cx| {
            pane.update(cx, |view, cx| view.poll_foreground(window, cx));
            pane.read(cx).git_status(cx).is_some()
        })
    });
    let counts = vcx.debug_bounds("sidebar-diff-0").unwrap();
    vcx.simulate_click(counts.center(), gpui::Modifiers::none());
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| matches!(&app.tabs[0].diff_overlay.as_ref().unwrap().load,
            DiffLoad::Ready(snapshot) if snapshot.files[0].hunks[0].lines[1].text == "before external edit"))
    });
    *body.lock().unwrap() = "after external edit".into();
    std::thread::sleep(Duration::from_millis(3100));
    settle(&mut vcx, |vcx| {
        app.update_in(vcx, |app, window, cx| {
            assert!(window.is_window_active());
            assert!(!pane.read(cx).focus_handle.is_focused(window), "diff owns focus");
            pane.update(cx, |view, cx| view.poll_foreground(window, cx));
            matches!(&app.tabs[0].diff_overlay.as_ref().unwrap().load,
                DiffLoad::Ready(snapshot) if snapshot.files[0].hunks[0].lines[1].text == "after external edit")
        })
    });
    app.read_with(&vcx, |app, _| {
        let DiffLoad::Ready(snapshot) = &app.tabs[0].diff_overlay.as_ref().unwrap().load else {
            unreachable!()
        };
        assert_eq!(
            snapshot.totals(),
            (2, 1),
            "fresh content can have identical totals"
        );
    });
    let id = pane.read_with(&vcx, |view, _| view.git_host_id());
    let epoch = app.read_with(&vcx, |_, cx| {
        cx.global::<crate::terminal::git_data::ScmData>()
            .epoch(id, &cwd)
    });
    assert!(
        epoch > 0,
        "the admitted native refresh advances diff freshness"
    );
    let admitted = status_queries.load(Ordering::SeqCst);
    for _ in 0..20 {
        app.update_in(&mut vcx, |_, window, cx| {
            pane.update(cx, |view, cx| view.poll_foreground(window, cx));
            cx.notify();
        });
        vcx.run_until_parked();
        app.update_in(&mut vcx, |_, _, cx| {
            assert!(!HostRegistry::ids(cx).contains(&id));
            assert!(HostRegistry::lookup(cx, id).is_some());
            assert!(
                cx.global::<GitStatusCache>().status_for(id, &cwd).is_some(),
                "ordinary renders retain live pane statistics"
            );
            assert_eq!(
                cx.global::<crate::terminal::git_data::ScmData>()
                    .epoch(id, &cwd),
                epoch,
                "ordinary renders retain the pane's diff freshness epoch"
            );
        });
    }
    app.update_in(&mut vcx, |app, window, cx| app.activate(1, window, cx));
    vcx.run_until_parked();
    assert_eq!(
        status_queries.load(Ordering::SeqCst),
        admitted,
        "poll ticks inside the interval do not query SSH again"
    );
    *body.lock().unwrap() = "edited while hidden".into();
    std::thread::sleep(Duration::from_millis(3100));
    app.update_in(&mut vcx, |_, window, cx| {
        pane.update(cx, |view, cx| view.poll_foreground(window, cx));
    });
    vcx.run_until_parked();
    assert_eq!(
        status_queries.load(Ordering::SeqCst),
        admitted,
        "hidden native panes do not poll"
    );
    app.update_in(&mut vcx, |app, window, cx| app.activate(0, window, cx));
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| {
        matches!(&app.tabs[0].diff_overlay.as_ref().unwrap().load,
            DiffLoad::Ready(snapshot) if snapshot.files[0].hunks[0].lines[1].text == "edited while hidden")
    })
    });
    // Another tab visit inside the three-second interval must refresh too.
    app.update_in(&mut vcx, |app, window, cx| app.activate(1, window, cx));
    *body.lock().unwrap() = "edited on a quick revisit".into();
    app.update_in(&mut vcx, |app, window, cx| app.activate(0, window, cx));
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| {
        matches!(&app.tabs[0].diff_overlay.as_ref().unwrap().load,
            DiffLoad::Ready(snapshot) if snapshot.files[0].hunks[0].lines[1].text == "edited on a quick revisit")
    })
    });

    let old_id = pane.read_with(&vcx, |view, _| view.git_host_id());
    pause.armed.store(true, Ordering::SeqCst);
    let _release_on_failure = ReleasePause(pause.clone());
    app.update_in(&mut vcx, |app, window, cx| {
        app.toggle_diff_overlay_at(old_id, cwd.clone(), Some("docs/new.md".into()), window, cx);
        pane.update(cx, |view, cx| view.refresh_git_status_now(cx));
    });
    settle(&mut vcx, |_| pause.started.load(Ordering::SeqCst) >= 3);
    app.update_in(&mut vcx, |_, _, cx| cx.notify());
    vcx.run_until_parked();
    app.update_in(&mut vcx, |_, _, cx| {
        assert!(!HostRegistry::ids(cx).contains(&old_id));
        assert!(HostRegistry::lookup(cx, old_id).is_some());
        let cache = cx.global::<GitStatusCache>();
        assert!(
            cache.status_for(old_id, &cwd).is_some(),
            "a render keeps statistics while their refresh is pending"
        );
        assert!(
            !cache.probe_due(old_id, &cwd, Duration::ZERO),
            "a render preserves the pending claim so another poll cannot duplicate it"
        );
        assert!(
            cx.global::<crate::terminal::git_data::ScmData>()
                .epoch(old_id, &cwd)
                > 0
        );
    });
    app.update_in(&mut vcx, |app, window, cx| app.activate(1, window, cx));
    daemons.0.shutdown(std::net::Shutdown::Both).unwrap();
    settle(&mut vcx, |vcx| {
        pane.read_with(vcx, |view, _| view.terminal.exited)
    });
    pause.release();
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| app.diff_probes_inflight.is_empty())
    });
    app.read_with(&vcx, |app, cx| {
        assert!(
            pane.read(cx).git_status(cx).is_none(),
            "disconnect clears statistics without waiting for another foreground poll"
        );
        assert!(
            cx.global::<GitStatusCache>()
                .status_for(old_id, &cwd)
                .is_none(),
            "late status cannot resurrect the old cache"
        );
        let overlay = app.tabs[0].diff_overlay.as_ref().unwrap();
        assert!(
            matches!(overlay.load, DiffLoad::Unavailable),
            "an inactive tab must discard its old snapshot and late diff replies"
        );
        assert!(
            overlay.preview.is_none(),
            "a late untracked read must not land after retirement"
        );
        assert!(overlay.file_views.is_empty());
        assert!(HostRegistry::lookup(cx, old_id).is_none());
        assert_eq!(
            cx.global::<crate::terminal::git_data::ScmData>()
                .epoch(old_id, &cwd),
            0,
            "disconnect still retires pane freshness"
        );
    });
    let new_body = Arc::new(Mutex::new("new connection".to_string()));
    let (fresh, mut fresh_daemon, _fresh_host) = app.update_in(&mut vcx, |app, window, cx| {
        let (parts, daemon) = quiet_test_shell_parts(80); // Reused daemon pane number.
        let fresh = cx.new(|cx| {
            TerminalView::from_native_ssh_parts(
                NativeSshParts {
                    terminal: parts.terminal,
                    pane_id: 80,
                    restored: false,
                    persist: Box::new(
                        serde_json::from_str(
                            r#"{"host":"build-box","port":22,"user":"me","auth_mode":"auto"}"#,
                        )
                        .unwrap(),
                    ),
                },
                window,
                cx,
            )
        });
        let id = fresh.read(cx).git_host_id();
        assert_ne!(id, old_id, "reconnect uses a fresh Git lifetime");
        assert!(
            fresh.read(cx).git_status(cx).is_none(),
            "new connection has no borrowed statistics before its cwd arrives"
        );
        assert!(
            !HostRegistry::ids(cx).contains(&id),
            "reconnection does not create another selectable machine"
        );
        let connection = HostRegistry::lookup(cx, id).expect("native pane route exists");
        let host: SharedHost = Arc::new(RemoteRepository {
            id,
            branch: "remote-new",
            body: Some(new_body.clone()),
            status_queries: None,
            connection: Some(connection),
            pause: None,
        });
        HostRegistry::insert_pane(cx, &host);
        assert!(!HostRegistry::ids(cx).contains(&id));
        app.tabs[0].pane = Pane::leaf(PaneSlot::Ready(fresh.clone()));
        cx.notify();
        (fresh, daemon, host)
    });
    crate::daemon::protocol::DaemonMsg::RemoteContext(Some(RemoteContext {
        kind: crate::daemon::protocol::RemoteKind::NativeSsh,
        argv: vec![],
        target: "build-box".into(),
    }))
    .encode(&mut fresh_daemon)
    .unwrap();
    crate::daemon::protocol::DaemonMsg::Cwd(cwd.clone())
        .encode(&mut fresh_daemon)
        .unwrap();
    settle(&mut vcx, |vcx| {
        app.update_in(vcx, |_, window, cx| {
            fresh.update(cx, |view, cx| view.poll_foreground(window, cx));
            fresh.read(cx).git_status(cx).is_some()
        })
    });
    let counts = vcx.debug_bounds("sidebar-diff-0").unwrap();
    vcx.simulate_click(counts.center(), gpui::Modifiers::none());
    settle(&mut vcx, |vcx| {
        app.read_with(vcx, |app, _| {
        matches!(&app.tabs[0].diff_overlay.as_ref().unwrap().load,
            DiffLoad::Ready(snapshot) if snapshot.branch == "remote-new" && snapshot.files[0].hunks[0].lines[1].text == "new connection")
    })
    });
    let fresh_id = fresh.read_with(&vcx, |view, _| view.git_host_id());
    app.update_in(&mut vcx, |app, window, cx| {
        app.close_other_tabs(1, window, cx)
    });
    fresh.read_with(&vcx, |view, cx| {
        assert!(
            view.git_status(cx).is_none(),
            "closing a tab retires Git even while another Entity reference holds its terminal"
        );
        assert!(
            cx.global::<GitStatusCache>()
                .status_for(fresh_id, &cwd)
                .is_none()
        );
    });
}
