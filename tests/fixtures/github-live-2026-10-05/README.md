# GitHub website computed styles

Captured on 2026-10-05 Asia/Shanghai through Chrome DevTools MCP, using a
separate signed-out browser context. The JSON files contain DOM computed
styles, viewport, theme attributes, capture time and the source URL. They
are observations of github.com, not output generated from ctty7's theme.

- `light.json`: 1280px viewport, Light default.
- `dark.json`: 1280px viewport, Dark default.
- `narrow-dark.json`: 600px viewport, Dark default.

The repository README page was also inspected. Its article uses the same
1012px content cap and 32px parent padding as the Markdown file preview.
The native card includes its padding, hence a 1076px total cap. The content
cap excludes GitHub's repository sidebar, tabs and other site navigation.

Default `data-a11y-link-underlines=true` applies in the signed-out context.
Links remain underlined with the same color on hover. Heading permalink
icons change opacity from 0 to 1 on heading hover. Task inputs are disabled.

The logged-in browser declared Monaspace Neon, but the signed-out default
uses `ui-monospace` and platform fallbacks. No account-specific font preference
was copied. A declared font family is not evidence that its font file loaded:
the default page only registered the Noto Sans Backtick Fix font face.

These captures do not prove native pixel parity. In particular, heading
permalink controls, native checkbox appearance, syntax token boundaries,
font rasterization and Mermaid output still require separate comparison.
