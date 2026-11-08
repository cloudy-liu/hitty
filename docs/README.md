# Hitty documentation

Start with the [product overview](index.mdx) and [installation guide](getting-started/installation.mdx).

- [Window and side panel](window/side-panel.mdx): Info, Outline, Git, Files and document viewing.
- [Agent status](agents/status.mdx) and [session restore](agents/sessions.mdx).
- [SSH](remote/ssh.mdx), [SFTP](remote/sftp.mdx) and [remote workspaces](remote/workspaces.mdx).
- [Git operations](git/source-control.mdx), [diffs](git/diffs.mdx) and [worktrees](git/worktrees.mdx).
- [Configuration](reference/configuration.mdx), [shortcuts](reference/keyboard-shortcuts.mdx) and [Markdown themes](customization/markdown-themes.mdx).
- [Contributor validation](testing/README.md), [release procedure](maintenance/hitty-release.md) and [issue tracker](agents/issue-tracker.md).
- [Source provenance](provenance/source-baseline.md) and [independent snapshot decision](adr/0001-independent-source-snapshot.md).

`docs.json` defines the documentation site's navigation. Current screenshots and
verification records live under `testing/`; executable test fixtures remain in
the repository's tests and scripts. Completed historical proposals and earlier
project release artifacts are recoverable from Git history rather than copied
into the current documentation tree.
