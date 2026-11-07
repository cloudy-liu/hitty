# Markdown table selection integration

The reader layout and aligned selection changes originally pinned sibling
component commits. Selecting either pin would omit the other change.

The aligned selection implementation was introduced by component commit
`e7711b27a74afaca714b3e52652e9c386195b6c8` and has since been merged into the
fork's `ctty7` integration branch. The application now pins
`9572b7fab9c0fcf73e34c6118f8a9f10f083f928`, which includes adaptive table
wrapping, external scrollbar ownership and aligned selection geometry. The
manifest and all three component lockfile sources use the same Git revision.

## Regression coverage

`markdown_table_text_can_be_selected_with_the_mouse` drags across the visible
number `118.3344` in right-aligned and centered table cells, in both directions.
Selection and clipboard contents must equal the complete number. Pointer
positions follow the table bounds and exclude the block's bottom gap, so the
regression also works with adaptive table layout. The drag stays clear of the
old left-aligned character bounds.

After merging the fork's current main, the Windows command
`cargo test --offline --locked --bin tty7-app ui::markdown_preview::tests -- --test-threads=1`
passes 20 tests with zero failures. One native-font test remains ignored because
the Windows test font collection does not include Noto Sans SC. The suite also
checks docked/filled reader scrollbar dragging, reaching the document end,
selection restoration, resources, Mermaid rendering and theme behavior.

Formatting, diff whitespace and the HostOps boundary check pass. This update
uses the committed Git dependency, without local path patches. Native desktop
mouse interaction was not manually repeated; pointer events and the copy
shortcut run through the GPUI application harness. Cross-platform results are
reported separately by CI.

The parallel settings suite also exercises the notification persistence test in
its own test process. Its real GPUI setters and disk reload assertions are
unchanged; the per-process scratch directory prevents unrelated settings tests
from overwriting the same config file. This fixes the shared-config failure
observed in the Linux integration run without skipping tests or retrying failed
assertions. The eight settings tests pass with 16 test threads.
