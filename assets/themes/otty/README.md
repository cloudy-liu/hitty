# Otty theme configuration profiles

Solarized Light is the only retained profile from the theme configuration
shipped with Otty 1.6.0-beta. The maintainer reviewed all 25 candidates and
removed the other 24. The original 13 Hitty presets remain available.
The picker displays `Solarized Light`; its internal `otty-solarized-light`
identifier stays stable so existing selections keep working.
`manifest.json` records the source and packaged YAML SHA-256 hashes, name,
mode and declared author. The original `.ottytheme` was checked against the
installed release before import.

Each YAML retains configuration values and author metadata. Original comments
and descriptive prose are omitted. This directory contains no Otty executable,
implementation source or fonts. Font stacks, font sizes, weights, cell-height
adjustments and blending declarations are retained as source metadata. They do
not override Hitty's interface font, scalable text hierarchy, terminal font,
font size or line height. Hitty's preferences and native HiDPI rendering apply
equally to imported and original themes.

The retained source has no declared author. The manifest preserves that fact
without inventing authors or license terms. This is an imported configuration,
not an original Hitty theme design. Existing third-party notices remain intact.

Regenerate from a local source export:

```powershell
python scripts/import-otty-themes.py --source-dir <exported-theme-directory>
python scripts/import-otty-themes.py --source-dir <exported-theme-directory> --check
```

The importer selects only `solarized-light.ottytheme`, even if the export
contains other profiles. It does not reintroduce rejected candidates.

Verify the packaged profile hashes without installing Otty:

```powershell
python scripts/import-otty-themes.py --check
```

Flat fields support Hitty's existing picker and palette APIs. The complete
nested `style` tree carries the source declarations through duplicate/edit
and live reload. Platform rendering limits and acceptance evidence are in
`docs/testing/otty-theme-fidelity.md`. The typography correction and clean
switching checks are recorded in `docs/testing/otty-hidpi-adaptation.md`.
