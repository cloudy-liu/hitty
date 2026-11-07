//! Project a complete Git patch into selected context without changing versions.
use std::ops::Range;

use crate::terminal::git_diff::{FileDiff, Hunk, LineKind};

/// Keep changed runs, three context lines, and explicitly revealed new lines.
pub(super) fn with_context(file: &FileDiff, revealed: &[Range<u32>]) -> FileDiff {
    let lines: Vec<_> = file.hunks.iter().flat_map(|h| &h.lines).collect();
    let mut keep = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if line.kind != LineKind::Context {
            let end = (i + 4).min(lines.len());
            keep[i.saturating_sub(3)..end].fill(true);
        } else if line
            .new_no
            .is_some_and(|n| revealed.iter().any(|r| r.contains(&n)))
        {
            keep[i] = true;
        }
    }
    let mut result = file.clone();
    result.hunks.clear();
    let mut i = 0;
    while i < lines.len() {
        if !keep[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && keep[i] {
            i += 1;
        }
        let body: Vec<_> = lines[start..i].iter().map(|l| (*l).clone()).collect();
        let old = body.iter().filter_map(|l| l.old_no).next().unwrap_or(0);
        let new = body.iter().filter_map(|l| l.new_no).next().unwrap_or(0);
        let old_count = body.iter().filter(|l| l.old_no.is_some()).count();
        let new_count = body.iter().filter(|l| l.new_no.is_some()).count();
        result.hunks.push(Hunk {
            header: format!("@@ -{old},{old_count} +{new},{new_count} @@"),
            lines: body,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::git_diff::{DiffLine, FileStatus};

    #[test]
    fn revealing_context_keeps_distant_changes_and_deletions() {
        let file = FileDiff {
            path: "example.rs".into(),
            old_path: None,
            status: FileStatus::Modified,
            added: 1,
            removed: 1,
            binary: false,
            truncated: None,
            hunks: vec![Hunk {
                header: String::new(),
                lines: (1..=80)
                    .map(|n| DiffLine {
                        kind: if n == 25 {
                            LineKind::Removed
                        } else if n == 65 {
                            LineKind::Added
                        } else {
                            LineKind::Context
                        },
                        old_no: (n != 65).then_some(n),
                        new_no: (n != 25).then_some(n),
                        text: format!("line {n}"),
                    })
                    .collect(),
            }],
        };
        let compact = with_context(&file, &[]);
        assert_eq!(compact.hunks.len(), 2);
        assert_eq!(compact.hunks[0].lines.first().unwrap().old_no, Some(22));
        let expanded = with_context(&file, &[2..22]);
        assert_eq!(expanded.hunks[0].lines.first().unwrap().new_no, Some(2));
        assert_eq!(expanded.added, 1);
        assert_eq!(expanded.removed, 1);
        assert!(
            expanded
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| l.kind == LineKind::Removed)
        );
        assert!(
            expanded
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| l.kind == LineKind::Added)
        );
        assert_eq!(file.hunks[0].lines.len(), 80);
    }
}
