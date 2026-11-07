# Markdown reader layout

The approved conversation calls for readable tables in narrow document panes and a consistent document scrollbar beside the Files panel.

- Prefer adaptive column widths and wrapped cell text. Keep row/column alignment and all cell content available, including links, inline code and CJK text.
- Use a horizontal table scrollbar when the columns cannot fit at a readable minimum width. Ordinary vertical wheel input continues scrolling the document; Shift-wheel moves an overflowing table horizontally.
- Keep the document's vertical scrollbar at the reading panel's right edge for both small and virtualized documents, in docked and filled layouts. The painted thumb ends 6 logical pixels before the panel edge, clearing the divider's 4-pixel extension.
- Preserve virtualized document scrolling, selection, heading navigation and reading-position restoration.
- Use 16/24/32 logical pixels of horizontal padding for the bundled GitHub theme below 600, from 600 through 900, and above 900 pixels of reading width. Preserve custom theme padding and the existing wide-document content cap.
- Test through GPUI layout and real pointer/scroll events. The previously measured 0/32/164-pixel viewport-edge differences are the regression baseline; a mockup is not acceptance evidence.

Review the implementation for correctness, conformance with this specification and unnecessary complexity. The pull request targets `cloudy-liu/ctty7:main`.
