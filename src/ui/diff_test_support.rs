use super::{FileDiff, git_diff};
use gpui::VisualTestContext;

pub(super) fn file(path: &str, lines: usize) -> FileDiff {
    git_diff::synthesize_added(
        path,
        (0..lines)
            .map(|line| format!("{path} line {line}\n"))
            .collect::<String>()
            .as_bytes(),
        &git_diff::DiffBudget::SINGLE_FILE,
    )
}

pub(super) fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        vcx.run_until_parked();
    }
}
