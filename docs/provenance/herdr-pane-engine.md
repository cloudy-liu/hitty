# Herdr-core pane engine references

The optional client in `crates/hitty-core/src/daemon/hc.rs` follows the framed
stdio client and binary-discovery patterns in **herdrdev/herdr-app**, licensed
under Apache-2.0. It is adapted to Hitty's synchronous pane IO and daemon
lifecycle; it is not an independently invented protocol.

The reference files used during the spike match revision
`f6fe216e3078225619621174e62b8b4d8cc56513` after normalizing text line endings:

| File | SHA-256 of upstream bytes |
| --- | --- |
| `src/hc.rs` | `1cdb9cc86209463508c6ed0450de78b2ff13717face14d0333449d186b0ee8bf` |
| `src/core_bin.rs` | `ec7d7be29f1c6c8cef0a95985295e8716110e8b86b9959317804d0f3bda4e14c` |
| `LICENSE` | `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4` |

Reference repository: `https://github.com/herdrdev/herdr-app`.
That revision has no `NOTICE` file; `THIRD-PARTY-NOTICES` is empty. No third-party
vendored code, dependencies, installer, or binary is copied from herdr-app.
The Apache-2.0 license is retained at the repository root and the adaptation
is credited in `NOTICE` and the client module's header.

Hitty-specific changes include profile-scoped terminal labels, the existing
prepared shell environment, compatibility gates, bounded startup waits,
PTY-trait adapters, shared reattachment state for all IO handles, lifecycle
snapshots, preserved daemon shutdown, pane-ID recovery, explicit close cleanup,
and isolated fake/real-core tests. Display colors use Hitty's appearance cache;
the GUI/daemon wire protocol and agent status pipeline are unchanged.

The protocol contract was checked against **herdrdev/herdr-core v0.1.15**:
`hc schema` from the installed executable, the framed protocol documentation,
and the tag's terminal status/message implementation. The implementation speaks
generation 1 and requires a core version of at least 0.1.15. These are reference
and compatibility facts, not new dependency pins or bundled software.
