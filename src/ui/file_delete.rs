use std::{cell::Cell, rc::Rc};

use gpui::prelude::*;
use gpui::{Context, Window, div};
use gpui_component::{
    Sizable as _, WindowExt as _, button::Button, checkbox::Checkbox, h_flex, v_flex,
};

use crate::core::config::Config;
use crate::ui::app::Tty7App;
use crate::ui::i18n::{L10nKey, t, t_plural};

impl Tty7App {
    /// The caller must capture the owning host and exact targets before calling.
    pub(crate) fn confirm_file_delete(
        &mut self,
        labels: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if labels.is_empty() {
            return;
        }
        if !cx.global::<Config>().confirm_file_delete {
            action(self, window, cx);
            return;
        }
        let app = cx.weak_entity();
        let checked = cx.new(|_| false);
        let action = Rc::new(action);
        let accepted = Rc::new(Cell::new(false));
        window.open_dialog(cx, move |dialog, _, cx| {
            let toggle = checked.clone();
            let accept_checked = checked.clone();
            let accept_app = app.clone();
            let action = action.clone();
            let accepted = accepted.clone();
            let accept = Rc::new(move |window: &mut Window, cx: &mut gpui::App| {
                if accepted.replace(true) {
                    return;
                }
                let opt_out = *accept_checked.read(cx);
                let _ = accept_app.update(cx, |app, cx| {
                    if opt_out {
                        app.update_config(cx, |config| config.confirm_file_delete = false);
                    }
                    action(app, window, cx);
                });
            });
            let button_accept = accept.clone();
            dialog
                .on_ok(move |_, window, cx| {
                    window.close_dialog(cx);
                    accept(window, cx);
                    false
                })
                .title(t_plural(L10nKey::FileDeleteConfirmTitle, labels.len(), &[]))
                .child(
                    v_flex()
                        .gap_2()
                        .child(t(L10nKey::FileDeleteConfirmBody))
                        .child(
                            div()
                                .id("file-delete-labels")
                                .max_h(gpui::px(220.))
                                .overflow_y_scroll()
                                .children(labels.iter().map(|label| div().child(label.clone()))),
                        )
                        .child(
                            Checkbox::new("file-delete-opt-out")
                                .debug_selector(|| "file-delete-opt-out".into())
                                .label(t(L10nKey::FileDeleteDontAskAgain))
                                .checked(*checked.read(cx))
                                .on_click(move |value, _, cx| {
                                    toggle.update(cx, |checked, cx| {
                                        *checked = *value;
                                        cx.notify();
                                    })
                                }),
                        ),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("file-delete-cancel")
                                .debug_selector(|| "file-delete-cancel".into())
                                .label(t(L10nKey::Cancel))
                                .small()
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("file-delete-confirm")
                                .debug_selector(|| "file-delete-confirm".into())
                                .label(t(L10nKey::Delete))
                                .small()
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    button_accept(window, cx);
                                }),
                        ),
                )
        });
    }
}
