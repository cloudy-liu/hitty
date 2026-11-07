# GitHub Light source editor appearance

Tracking issue: [cloudy-liu/ctty7#89](https://github.com/cloudy-liu/ctty7/issues/89).

Background update in [cloudy-liu/ctty7#186](https://github.com/cloudy-liu/ctty7/pull/186):
the editor and gutter now use the selected application theme's opaque base color.
This supersedes the fixed editor-background requirements below. GitHub Light
and Atom One Dark still supply syntax and editing control colors according to
the application's resolved appearance.

## Problem Statement

The source editor previously used Atom One Light in every light application
appearance. The user prefers GitHub's default light code editor colors and
wants exactly two automatic source palettes: GitHub Light and Atom One Dark.

## Solution

Use GitHub Light for every resolved light application appearance and retain
Atom One Dark for every resolved dark appearance. Replace the complete light
editor palette, including its white background, syntax and editing controls.
Show the active palette in settings and preserve the existing automatic
appearance and independent source typography behavior.

## User Stories

1. As a light-theme user, I want GitHub Light in the source editor, so that code uses GitHub's familiar default light colors.
2. As a dark-theme user, I want Atom One Dark, so that my existing dark code appearance stays familiar.
3. As a user choosing any light application theme, I want the same GitHub Light editor, so that application presets do not create additional source palettes.
4. As a user choosing any dark application theme, I want the same Atom One Dark editor, so that there are only two source palettes.
5. As a source reader, I want a white light editor background and matching gutter, so that the complete editor follows GitHub's light style.
6. As a source reader, I want GitHub keyword, string, function, constant and identifier colors, so that replacing the light theme changes syntax as well as the background.
7. As a user selecting or navigating code, I want matching selection, caret, current-line and line-number colors, so that editing controls belong to the selected palette.
8. As a user searching source, I want visible ordinary and current matches in both appearances, so that the existing search contrast fix remains effective.
9. As a settings user, I want the current source palette named GitHub Light or Atom One Dark, so that settings describe the actual behavior.
10. As a multilingual settings user, I want translated explanations and searchable theme names, so that I can find and understand editor colors.
11. As a system-theme user, I want source appearance to follow the application's effective appearance, so that the editor switches with the application.
12. As a user disabling system following, I want my selected application appearance respected, so that the editor does not follow the system independently.
13. As a user previewing or canceling an application theme, I want source colors to update and restore with the preview, so that previews are reversible.
14. As a custom-theme user, I want resolved appearance to select the source palette, so that misleading preset names do not choose colors.
15. As a user with wallpaper, gradients or transparency, I want opaque source and gutter backgrounds, so that code contrast remains predictable.
16. As a user with multiple files or editor layouts, I want consistent source colors, so that file location and document fill do not affect the palette.
17. As a user with unsaved edits, I want text, dirty state, cursor, selection, scrolling and undo retained during appearance changes, so that switching themes cannot interrupt editing.
18. As a reader of supported or embedded languages, I want GitHub light colors without losing existing language classification, so that injections and language-specific roles remain readable.
19. As a Markdown author, I want GitHub Light in light source mode, so that source editing follows the same source-editor contract.
20. As an existing user, I want obsolete editor theme preferences ignored safely, so that this change needs no configuration migration.
21. As a typography user, I want independently saved font family, size and line height, so that replacing colors does not reset text preferences.
22. As a maintainer, I want attributed, pinned light color sources and offline regeneration, so that runtime rendering never depends on a theme download.
23. As a reviewer, I want a tracking issue, behavior verification and a visual PR outline, so that the change can be reviewed against the agreed requirement.

## Implementation Decisions

1. Resolve the complete editor palette from the application's effective light or dark mode through the existing source appearance module. Keep the automatic two-palette contract and read-only active-palette settings row.
2. Replace the Atom One Light palette and its generation inputs with GitHub default light file-viewer syntax and editable control tokens captured from the current published stylesheet. Preserve source URL, capture date, license and a content fingerprint. Retain the pinned Atom One Dark source and generated dark bytes.
3. Use GitHub's white background and dark foreground, together with its file-viewer syntax and editable control colors. Map those roles onto the existing Tree-sitter capture vocabulary, including language-qualified captures. Keep syntax registrations isolated from the Markdown reader.
4. Retain the existing search contrast calculation. Ordinary matches use the editor contrast budget; the current match uses the application accent with the existing fallback.
5. Keep source and gutter backgrounds opaque. Apply colors to the existing input instance without replacing document state or introducing another system appearance listener.
6. Update English, Chinese and Japanese settings text, active-theme names, customization documentation and reference documentation. Remove obsolete Atom One Light runtime assets and generation rules.
7. Keep independent source typography and retired editor-theme compatibility. No new configuration fields, application themes, dependencies or runtime downloads are needed.

Source typography retains `editor_font_family`, `editor_font_size` and
`editor_line_height` as independent saved preferences. Font size supports
8 to 72 px with a 13 px default; line height supports 1 to 3 times the text
size with a 1.5 default. Numeric controls submit on Enter or blur, preserve
in-progress input during unrelated refreshes and bound invalid values.
Font-size shortcuts change focused source text and otherwise retain terminal
behavior.

The retired `editor_theme` key is ignored on load and omitted on save,
including legacy fixed palettes, unknown strings and malformed values.
Other valid preferences remain intact. Theme names do not select palettes:
One Dark Pro remains an application preset and uses Atom One Dark for source.

## Testing Decisions

1. Prefer the existing application-window tests as the main behavior boundary. Exercise theme selection, settings, system appearance, preview and cancellation, opaque backgrounds and retention of editing state.
2. Reuse the actual production highlighter tests to compare representative GitHub light colors with independently recorded website values. Retain the Atom One Dark TextMate reference corpus, embedded-language regressions and shared-language isolation checks.
3. Compare source background, foreground, gutter, current line, line numbers, selection and caret against the captured GitHub source. Verify dark source colors retain their previous authored values.
4. Check offline regeneration, formatting, whitespace, host-boundary rules and a locked Windows build. Run targeted tests during implementation and the full locked workspace suite after the final implementation.
5. Distinguish native screenshots from headless application-window assertions and palette illustrations. Record unperformed native or other-platform visual checks explicitly.

## Out of Scope

- Additional editor palettes, a separate editor theme picker, fixed source appearance, custom editor theme imports and per-file themes.
- Application or terminal theme changes, Markdown reading styles, ordinary input fields and diff styling.
- Editor replacement, language-server semantic tokens, bracket-pair coloring and new language grammars.
- Reproducing every TextMate selector or every browser editor feature.

## Further Notes

This specification consolidates the source appearance and typography contract
from [PR #86](https://github.com/cloudy-liu/ctty7/pull/86) with the GitHub Light
replacement in [PR #90](https://github.com/cloudy-liu/ctty7/pull/90).
Both are merged; implementation history remains available in those PRs.
Final results and verification limits are recorded in the
[verification record](editor-github-light-verification.md).

The user's follow-up screenshot and alignment request establish GitHub's default
light file viewer as the syntax reference. Use the pinned prettylights syntax
tokens, including gray comments (`#59636E`) and deep purple functions (`#6639BA`),
and the website selection token. Match Rust variable, parameter, type, field,
reference and variant classification to the actual file-viewer reference sample.
Verify its complete 70 color ranges with the production highlighter. Retain
independent source typography and editable control behavior; dark parent-style
fallbacks must preserve Atom One Dark's authored colors.
