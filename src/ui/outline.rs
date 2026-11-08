use gpui::{
    AnyElement, Context, Entity, EntityId, SharedString, Window, div, prelude::*, px, rems,
};
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use std::collections::HashSet;

use crate::ui::{
    app::HittyApp,
    i18n::{L10nKey, t},
};

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub(crate) struct Heading {
    pub depth: u8,
    pub text: String,
    pub anchor: String,
}

#[derive(Default)]
pub(crate) struct OutlineState {
    markdown: Option<(EntityId, SharedString)>,
    headings: Vec<Heading>,
    loading: bool,
    #[cfg(any(windows, target_os = "macos"))]
    html_key: Option<(EntityId, EntityId, u64)>,
    #[cfg(any(windows, target_os = "macos"))]
    html_headings: Vec<Heading>,
    #[cfg(any(windows, target_os = "macos"))]
    html_pending: bool,
}

// The pinned renderer does not expose its heading AST. Use its exact parser
// and anchor convention, then confirm each anchor against the rendered AST.
fn markdown_headings(source: &str) -> Vec<Heading> {
    fn visit(
        node: &markdown::mdast::Node,
        used: &mut HashSet<String>,
        headings: &mut Vec<Heading>,
    ) {
        if let markdown::mdast::Node::Heading(heading) = node {
            let text = node.to_string().replace('\n', " ");
            let base: String = text
                .trim()
                .to_lowercase()
                .chars()
                .filter_map(|c| {
                    if c.is_whitespace() {
                        Some('-')
                    } else if c.is_alphanumeric() || c == '-' || c == '_' {
                        Some(c)
                    } else {
                        None
                    }
                })
                .collect();
            let mut anchor = base.clone();
            let mut suffix = 1;
            while !used.insert(anchor.clone()) {
                anchor = format!("{base}-{suffix}");
                suffix += 1;
            }
            headings.push(Heading {
                depth: heading.depth,
                text,
                anchor,
            });
        } else if let Some(children) = node.children() {
            for child in children {
                visit(child, used, headings);
            }
        }
    }
    let mut headings = Vec::new();
    if let Ok(node) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) {
        visit(&node, &mut HashSet::new(), &mut headings);
    }
    headings
}

impl HittyApp {
    pub(crate) fn render_panel_outline(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.panel_title(t(L10nKey::PanelOutlineTitle), None, None, window, cx);
        if self.document_front() == Some(crate::ui::app::OverlayTop::Code) {
            let document = self
                .tab_code()
                .and_then(|code| code.active_file())
                .map(|file| {
                    (
                        file.path.clone(),
                        file.input.entity_id(),
                        file.reading.clone(),
                        file.preview,
                    )
                });
            if let Some((path, owner, Some(reading), true)) = document {
                let source = reading.read(cx).text.read(cx).source();
                self.sync_markdown_outline(owner, source, cx);
                let headings = self.right_panel.outline.headings.clone();
                let mut body = v_flex().px_2().py_1().gap_1();
                body = body.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_size(rems(12. / 16.))
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        ),
                );
                for heading in headings {
                    if reading
                        .read(cx)
                        .text
                        .read(cx)
                        .anchor_index(&heading.anchor)
                        .is_none()
                    {
                        continue;
                    }
                    let target = reading.clone();
                    let anchor = heading.anchor.clone();
                    let selector = format!("outline-heading-{}", heading.anchor);
                    body = body.child(
                        h_flex()
                            .id(SharedString::from(selector.clone()))
                            .debug_selector(move || selector.clone())
                            .items_center()
                            .gap_2()
                            .pl(px(8. + f32::from(heading.depth.saturating_sub(1)) * 12.))
                            .pr_2()
                            .py_1()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().secondary))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(rems(10. / 16.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("H{}", heading.depth)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(rems(13. / 16.))
                                    .child(heading.text),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if app
                                    .tab_code()
                                    .and_then(|code| code.active_file())
                                    .is_some_and(|file| {
                                        file.input.entity_id() == owner && file.preview
                                    })
                                {
                                    target.update(cx, |preview, cx| {
                                        preview.navigate_anchor(anchor.clone(), cx)
                                    });
                                }
                            })),
                    );
                }
                if self.right_panel.outline.headings.is_empty() {
                    body = body.child(self.panel_empty(
                        t(if self.right_panel.outline.loading {
                            L10nKey::PanelLoading
                        } else {
                            L10nKey::PanelOutlineEmpty
                        }),
                        None,
                        cx,
                    ));
                }
                return self.panel_scroll(body.into_any_element(), title);
            }
            #[cfg(any(windows, target_os = "macos"))]
            if let Some(html) = self.editor.html.clone() {
                let current = self
                    .tab_code()
                    .and_then(|code| code.active_file())
                    .is_some_and(|file| {
                        file.preview
                            && file.input.entity_id() == html.read(cx).owner
                            && file.path == html.read(cx).path
                    });
                if current && html.read(cx).ready() {
                    self.sync_html_outline(&html, cx);
                    let generation = html.read(cx).outline_generation();
                    let owner = html.read(cx).owner;
                    let entries = self.right_panel.outline.html_headings.clone();
                    let mut body = v_flex().px_2().py_1().gap_1();
                    for (ix, heading) in entries.iter().enumerate() {
                        let heading = heading.clone();
                        let target = html.clone();
                        body = body.child(
                            div()
                                .id(("outline-html", ix))
                                .debug_selector(move || format!("outline-html-{ix}"))
                                .pl(px(8. + f32::from(heading.depth.saturating_sub(1)) * 12.))
                                .pr_2()
                                .py_1()
                                .rounded(px(5.))
                                .text_size(rems(13. / 16.))
                                .truncate()
                                .cursor_pointer()
                                .hover(|s| s.bg(cx.theme().secondary))
                                .child(heading.text.clone())
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    if !app
                                        .tab_code()
                                        .and_then(|code| code.active_file())
                                        .is_some_and(|file| {
                                            file.preview && file.input.entity_id() == owner
                                        })
                                    {
                                        return;
                                    }
                                    let result =
                                        target.read(cx).navigate_heading(&heading, generation, cx);
                                    if let Ok(receiver) = result {
                                        cx.spawn_in(window, async move |_app, cx| {
                                            if html_reply(receiver).await.as_deref()
                                                == Some("false")
                                            {
                                                let _ = cx.update(|window, cx| {
                                                    use gpui_component::WindowExt as _;
                                                    window.push_notification(
                                                        t(L10nKey::PanelOutlineUnavailable),
                                                        cx,
                                                    );
                                                });
                                            }
                                        })
                                        .detach();
                                    }
                                })),
                        );
                    }
                    if entries.is_empty() {
                        body =
                            body.child(self.panel_empty(t(L10nKey::PanelOutlineEmpty), None, cx));
                    }
                    return self.panel_scroll(body.into_any_element(), title);
                }
            }
            return self.panel_scroll(
                self.panel_empty(t(L10nKey::PanelOutlineEmpty), None, cx),
                title,
            );
        }
        if self.document_front().is_some() {
            return self.panel_scroll(
                self.panel_empty(t(L10nKey::PanelOutlineEmpty), None, cx),
                title,
            );
        }
        let leaf = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.detail_pane(window, cx));
        if let Some(leaf) = leaf.as_ref() {
            if leaf.read(cx).agent_turns().is_empty() {
                let commands = leaf.read(cx).terminal.command_entries();
                if !commands.is_empty() {
                    let mut body = v_flex().px_2().py_1().gap_1();
                    for entry in commands {
                        let target = leaf.clone();
                        let valid = entry.row.is_some();
                        let id = entry.id;
                        body = body.child(
                            div()
                                .id(("outline-command", id as usize))
                                .debug_selector(move || format!("outline-command-{id}"))
                                .px_2()
                                .py_1()
                                .rounded(px(5.))
                                .text_size(rems(13. / 16.))
                                .text_color(if valid {
                                    cx.theme().foreground
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .when(valid, |el| {
                                    el.cursor_pointer().hover(|s| s.bg(cx.theme().secondary))
                                })
                                .child(div().truncate().child(entry.text))
                                .child(
                                    div()
                                        .text_size(rems(11. / 16.))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(match (entry.done, entry.exit) {
                                            (true, Some(code)) => format!("exit {code}"),
                                            (true, None) => t(L10nKey::PanelAgentDone).into(),
                                            (false, _) => t(L10nKey::PanelAgentWorking).into(),
                                        }),
                                )
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    let current = app
                                        .tabs
                                        .get(app.active)
                                        .and_then(|tab| tab.detail_pane(window, cx));
                                    if current.as_ref().map(|pane| pane.entity_id())
                                        != Some(target.entity_id())
                                    {
                                        return;
                                    }
                                    if !target.update(cx, |view, cx| view.scroll_to_command(id, cx))
                                    {
                                        use gpui_component::WindowExt as _;
                                        window.push_notification(
                                            t(L10nKey::PanelOutlineUnavailable),
                                            cx,
                                        );
                                    }
                                })),
                        );
                    }
                    return self.panel_scroll(body.into_any_element(), title);
                }
            }
        }
        let body = self
            .turns_section(leaf.as_ref(), cx)
            .unwrap_or_else(|| self.panel_empty(t(L10nKey::PanelOutlineEmpty), None, cx));
        self.panel_scroll(body, title)
    }

    fn sync_markdown_outline(
        &mut self,
        owner: EntityId,
        source: SharedString,
        cx: &mut Context<Self>,
    ) {
        let key = (owner, source.clone());
        if self.right_panel.outline.markdown.as_ref() == Some(&key) {
            return;
        }
        self.right_panel.outline.markdown = Some(key.clone());
        self.right_panel.outline.headings.clear();
        self.right_panel.outline.loading = true;
        let task = cx.background_spawn(async move { markdown_headings(&source) });
        cx.spawn(async move |app, cx| {
            let headings = task.await;
            let _ = app.update(cx, |app, cx| {
                if app.right_panel.outline.markdown.as_ref() != Some(&key) {
                    return;
                }
                app.right_panel.outline.headings = headings;
                app.right_panel.outline.loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn sync_html_outline(
        &mut self,
        html: &Entity<super::html_preview::HtmlPreview>,
        cx: &mut Context<Self>,
    ) {
        let key = (
            html.entity_id(),
            html.read(cx).owner,
            html.read(cx).outline_generation(),
        );
        if self.right_panel.outline.html_key != Some(key) {
            self.right_panel.outline.html_key = Some(key);
            self.right_panel.outline.html_headings.clear();
            self.right_panel.outline.html_pending = false;
        }
        if self.right_panel.outline.html_pending {
            return;
        }
        let Ok(receiver) = html.read(cx).outline_headings(cx) else {
            return;
        };
        self.right_panel.outline.html_pending = true;
        cx.spawn(async move |app, cx| {
            let headings = html_reply(receiver)
                .await
                .filter(|json| json.len() <= 2 * 1024 * 1024)
                .and_then(|json| serde_json::from_str::<Vec<Heading>>(&json).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|h| (1..=6).contains(&h.depth))
                .take(2000)
                .collect::<Vec<_>>();
            let _ = app.update(cx, |app, cx| {
                if app.right_panel.outline.html_key != Some(key) {
                    return;
                }
                if app.right_panel.outline.html_headings != headings {
                    app.right_panel.outline.html_headings = headings;
                    cx.notify();
                }
            });
            smol::Timer::after(std::time::Duration::from_millis(750)).await;
            let _ = app.update(cx, |app, cx| {
                if app.right_panel.outline.html_key != Some(key) {
                    return;
                }
                app.right_panel.outline.html_pending = false;
                if app.right_panel_visible
                    && app.right_panel_tab == crate::core::config::RightPanelTab::Outline
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(any(windows, target_os = "macos"))]
async fn html_reply(receiver: smol::channel::Receiver<String>) -> Option<String> {
    use smol::future::FutureExt as _;
    async { receiver.recv().await.ok() }
        .or(async {
            smol::Timer::after(std::time::Duration::from_secs(3)).await;
            None
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_outline_handles_nested_unicode_duplicates_and_code_fences() {
        let source = "# **标题** `code`\n\n## 标题 code\n\n> ### Nested\n\n```md\n# Not a heading\n```\n\nSetext\n------\n";
        let entries = markdown_headings(source);
        assert_eq!(
            entries
                .iter()
                .map(|h| (h.depth, h.text.as_str(), h.anchor.as_str()))
                .collect::<Vec<_>>(),
            [
                (1, "标题 code", "标题-code"),
                (2, "标题 code", "标题-code-1"),
                (3, "Nested", "nested"),
                (2, "Setext", "setext")
            ]
        );
    }

    #[gpui::test]
    fn command_outline_click_scrolls_without_switching_sessions(cx: &mut gpui::TestAppContext) {
        use crate::daemon::protocol::DaemonMsg;
        use crate::ui::{
            app::{Tab, test_window},
            pane::{Pane, PaneSlot},
        };
        let (app, mut vcx) = test_window::harness(cx);
        let (view, mut stream) = app.update_in(&mut vcx, |app, window, cx| {
            let (view, stream) = crate::terminal::view::quiet_test_pane(1, window, cx);
            app.tabs
                .push(Tab::new(Pane::leaf(PaneSlot::Ready(view.clone()))));
            app.active = app.tabs.len() - 1;
            app.right_panel_visible = true;
            app.right_panel_tab = crate::core::config::RightPanelTab::Outline;
            cx.notify();
            (view, stream)
        });
        let mut output = b"\x1b]133;A\x07$ example\r\n\x1b]133;C;example\x07".to_vec();
        output.extend_from_slice("line\r\n".repeat(100).as_bytes());
        output.extend_from_slice(b"\x1b]133;D;0\x07");
        DaemonMsg::Output(output).encode(&mut stream).unwrap();
        let before = app.read_with(&vcx, |app, _| (app.tabs.len(), app.active));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let target = loop {
            vcx.run_until_parked();
            app.update_in(&mut vcx, |_, _, cx| cx.notify());
            if let Some(bounds) = vcx.debug_bounds("outline-command-0") {
                break bounds;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        vcx.simulate_click(target.center(), gpui::Modifiers::none());
        vcx.run_until_parked();
        app.read_with(&vcx, |app, _| {
            assert_eq!((app.tabs.len(), app.active), before)
        });
        view.read_with(&vcx, |view, _| {
            assert!(view.terminal.term.lock().grid().display_offset() > 0)
        });
    }
}
