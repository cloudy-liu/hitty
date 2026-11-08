# Hitty terminology

**Hitty**: the independent terminal workspace product built on tty7. Its product
name is Hitty; `hitty` is its repository and command name.

**Source baseline**: the reviewed source snapshot recorded in
[provenance](docs/provenance/source-baseline.md). Imported implementation is not
a rewrite from scratch.

**Session**: a terminal task represented in the left sidebar. A session may own
multiple panes and document tabs.

**Document tab**: an opened file or diff within its owning session. Opening one
does not create or select a different session in the left sidebar.

**Right panel**: the contextual Info, Outline, Git and Files views beside the
terminal/document area. Mode selection is independent of the session list.

**Outline**: navigation entries for the content being viewed: document headings,
Agent turns or recorded shell commands. Selecting an entry changes reading
position; it does not execute a command.

**Tab avatar**: the icon identifying a session in navigation. In a split tab it
represents the focused pane, including when that tab moves into the background.

**Host application**: a terminal application, such as Herdr, that manages its own
sessions inside a Hitty pane and can launch coding agents.

**Nested agent**: an agent running inside a host application's session; its
status and notification behavior remains separate from the host avatar.
