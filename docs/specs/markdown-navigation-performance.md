# Markdown navigation and performance

The acceptance fixture is `tests/fixtures/markdown/large-navigation.md`: 101,027
UTF-8 bytes, 1,873 lines, six generated Mermaid diagrams, tables, code blocks,
and a final `DOCUMENT_END_MARKER`. It contains real Markdown and Mermaid source.

## Required behavior

- Clicking an inline diagram opens a larger reading area. Dragging an inline diagram
  must not accidentally open it.
- The expanded view follows GitHub's diagram dialog: retain the SVG's natural
  width (capped by the available width), preserve its aspect ratio, and start
  at the top. Never shrink a tall diagram to fit the viewport height. Content
  taller than the viewport has a visible, draggable vertical scrollbar.
  Scaled content wider than the viewport has a horizontal scrollbar. The
  scroll extent follows zoom, and both scrolling and dragging a scrollbar
  can reach the actual final node without changing the reading size.
- Ordinary wheel input scrolls an expanded diagram without changing its zoom.
  Modifier-wheel input zooms around the pointer. Shift-wheel supports horizontal
  movement with a standard mouse. Unzoomed inline diagrams let the document scroll.
- Reset restores the initial reading size and position. Home scrolls to the
  top of an expanded diagram without resetting zoom; End reaches the bottom.
  Zoom buttons, keyboard navigation, and Escape remain usable.
- Expanded direction buttons are disabled when their axis has no scrollable
  overflow. Opening a diagram does not itself imply horizontal overflow;
  zooming beyond the available width enables left/right navigation.
- The wide diagram's `WIDE_END_24`, tall diagram's `TALL_END_36`, and long sequence's
  final message must be reachable after enlarging the diagram.
- A diagram update invalidates its containing reading view rather than forcing
  every view in the window to rerender. Expanded dragging must visibly repaint.
- Long Markdown uses the component's existing virtual list. Offscreen headings
  are addressable before being painted. Home, End, page navigation, selection,
  source editing, theme changes, and document resizing continue to work.
- Every performance comparison uses the same fixture, viewport, DPI, build
  optimization, warmup, and event sequence. Both event handling and visible
  transform/scroll changes are checked so an unchanged cached frame cannot pass.

## Evidence

Run the ignored `markdown_large_document_frame_profile` benchmark for repeatable
GPUI test-platform CPU measurements, and the optional `profile_output` mode of
the native `--markdown-visual` fixture for Windows font and image measurements.
Use three fresh native processes per revision, with 100 measured frames after
10 warmup frames for each interaction. Record median, P95, maximum, full SVG
generation readiness, binary hashes, and the exact build configuration.

These measure CPU event dispatch and GPUI layout/prepaint/paint. They exclude
presentation and GPU execution; they must not be reported as displayed FPS.
Validate scrolling, dragging, zooming and distant markers with Computer Use as
well as the automated checks. Keep original before/after screenshots on the PR.

GitHub's live Markdown diagram dialog is the product reference. On 2026-10-14,
the browser observation of this fixture's tall diagram showed a 340.5 × 4,672
SVG inside a 1,152 × 576 dialog, with `overflow-y: auto`. End reached the last
node at scrollTop 4,160 (scrollHeight 4,735; clientHeight 575). The wide diagram
was capped to the available width. These are direct observations of
<https://github.com/cloudy-liu/ctty7/blob/9b0fd9a3d66214788b32ccacf170c3c3432753c9/tests/fixtures/markdown/large-navigation.md>.
The modifier-wheel and keyboard bindings above are ctty7's explicit behavior.
