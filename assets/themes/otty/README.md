# Otty theme configuration profiles

These 25 profiles are transcribed from the theme configuration shipped with
Otty 1.6.0-beta. The original local `.ottytheme` definitions were checked
against the installed release before import. `manifest.json` records the
source SHA-256, packaged YAML SHA-256, name, mode and declared author.

Each YAML retains configuration values and author metadata. Original comments
and descriptive prose are omitted. This directory contains no Otty executable,
implementation source or fonts. Font names request installed faces; a missing
face falls back to Hitty's configured font.

The source themes credit Otty, Abner, Typora and arcticicestudio where an author is
declared; other entries have no declared author. The manifest preserves that
information without inventing authors or license terms. These are imports,
not original Hitty theme designs. Existing third-party notices remain intact.

Regenerate from a local source export:

```powershell
python scripts/import-otty-themes.py --source-dir <exported-theme-directory>
python scripts/import-otty-themes.py --source-dir <exported-theme-directory> --check
```

Verify the packaged profile hashes without installing Otty:

```powershell
python scripts/import-otty-themes.py --check
```

Flat fields support Hitty's existing picker and palette APIs. The complete
nested `style` tree carries the source declarations through duplicate/edit
and live reload. Platform rendering limits and acceptance evidence are in
`docs/testing/otty-theme-fidelity.md`.
