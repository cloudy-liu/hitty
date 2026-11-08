# Scratch group Inbox icon

On 2026-10-31, Local Computer Use captured the running Windows GPUI application
before and after the Scratch Inbox icon was combined with SSH host groups.
Both captures use the same deterministic six-session fixture, light theme,
Repo sidebar grouping, active `Fix login` session, 1200 x 660 logical viewport
and 100% display scale. No live SSH connection is opened.

| Phase | Source | Screenshot | Binary SHA-256 |
| --- | --- | --- | --- |
| Before | SSH host groups source `25772ed0` | [before](images/sidebar-scratch-inbox/before-windows.jpg) | `9c3d3a5ea8f4316a9249c732a6bff4178942b525fa6ae7fe1cf37da05e51d507` |
| After | combined source `0ad8166f` | [after](images/sidebar-scratch-inbox/after-windows.jpg) | `9f9b7ef34dc57a0d4fe37baef318c732c69860d1e5cab82b653e8704dca942d3` |

Before, Scratch has an arrow and label. After, its Inbox icon appears between
them; the Project and SSH host groups retain their existing markers. Both
captures set the first local tab's fixture `sidebar_group` cache to
`Some(PathBuf::from("C:/visual/Project"))` immediately after `Tab::new` so a
Project group appears alongside Scratch. This changes fixture data only, not
the production grouping or rendering code.

Build each source with `cargo build --locked --features markdown-visual-tests
--bin tty7-app`, then launch it with an isolated configuration directory and
`--ssh-tab-visual` options containing `{"sidebar": true, "hold_seconds": 7200,
"output": "<unused-on-Windows>"}`. Capture the window after
`NATIVE_SSH_TABS_READY`. The [SSH group record](ssh-host-groups.md) documents
the fixture and its baseline overlay in more detail.

The image SHA-256 values are
`3b0aef9c6c52ad18935df2532436507017dabcc93575bf791b5087a60625567b`
and `861c357ca62c92f7f0fdb5ced3f73fccf95e70a56b85a79fc5ca16100e89e236`.
macOS/Linux appearance and a live SSH connection were not inspected manually.
