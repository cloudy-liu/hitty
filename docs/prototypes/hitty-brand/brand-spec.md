# Hitty prototype assets and decisions

## Source

- Existing brand studies: `design-demos/a-roulette-data-humanism.html`,
  `design-demos/b-reference.html`, `design-demos/c-designer.html`.
- Current application assets inspected: `../../../assets/app-icon.svg` and
  `../../../assets/tray.svg`.
- Current attention implementation inspected: `../../../src/ui/tray/taskbar.rs`.
- User-approved task: create an HTML brand prototype directly, continuing the
  Hitty conversation. On 2026-11-08 the user selected direction 08 and confirmed
  `hitty` as the new brand. Production rollout is being assessed separately.

## Visual system

- Paper `#F5F1E8`, ink `#17171F`, attention blue `#0078D4`.
- A uses the existing rose `#E0628E`; B uses a dark terminal screen; C uses the
  existing pixel letterforms. All three inherit geometry from the prior studies.
- Wordmarks are inline vector paths. UI labels use installed fonts for offline use.
- No external brands, fetched fonts, third-party logos, photographs, or analytics.
- New delivery is `index.html`, a self-contained file supporting local file URLs.

## Interaction and meaning

- Switching direction updates the hero, taskbar, title bar, size samples, tray,
  favicon, and lockups. Alternatives remain inspectable; direction 08 is approved.
- The taskbar mark has a neutral i dot; an independent blue attention overlay
  appears only in the simulated Waiting state. Standalone brand art may use color.
- Agent states are illustrated in the pane heading and separate status controls.
- UI scenes and task text are explicitly marked as prototypes, not live app output.
- The overlay position and tray geometry are design proposals, not an assertion
  that a specific platform always renders them this way.
- User can export a self-contained icon or wordmark SVG for the selected direction.

## Round 02: soft terminal marks

- `terminal-studies.html` develops the user's preference for A into six alternatives:
  hi cursor, prompt plus hi, a light window, prompt plus i, classic prompt/cursor,
  and two session rows.
- The user explicitly requested an additional fusion with the current app icon.
  Study 07 keeps overlapping panes, replaces the front glyph with rounded hi, moves
  green to a small cursor, and softens the rear pane to pink `#E8ADC4`.
- The current asset's original SVG geometry is embedded for direct comparison.
  Displaying its canvas cropped to the icon bounds matches the other specimens.
- All seven include full-size art, CSS-pixel specimens, taskbar mockups, monochrome
  inspection and SVG export. Tiny tray marks are simplified proposals.

## Integrated delivery

Per the user's correction, all seven alternatives are now integrated directly into
the original `index.html` and drive every existing scene there. The original A/B/C
group is available through an in-page selector. The separate Round 02 study remains
an archive. The original file was backed up as `index-round1.html` before integration.

## Selected direction 08 — 2026-11-08

The user selected keeping the existing terminal logo with the new hitty name.
`current-logo` is presented first and selected initially in `index.html`, alongside
the previous seven terminal directions and A/B/C. It embeds the original app-icon
geometry and palette unchanged; only its empty outer canvas margin is cropped for
consistent comparisons. The wordmark remains rounded Hitty, and the template tray
uses the original `assets/tray.svg` geometry with unique mask IDs and currentColor.
The visual direction is approved. This prototype does not itself change production
executables, installation identity, configuration, or the GitHub repository.
