# Winlane icons

Both marks are drawn by `scripts/make-icon.swift`, so the identity is
reproducible and lives in version control. Nothing here depends on an external
image generator.

The app icon is a pack of six glossy tool chips — violet, azure, cyan, amber,
orange and coral — on a deep violet enamel slab, with the last chip lifted and
starred so the pack looks alive rather than like a plain grid. Bold blocks of
colour with one smooth gloss keep it readable from 16 to 1024 pixels.

The menu-bar mark reduces the same pack to an 18 × 18 point two-by-two grid with
the current tool filled in. It is embedded in the executable and rendered as an
AppKit template image, so macOS supplies the appropriate foreground colour.

- `AppIcon.png`: 1024 pixel master artwork, written by the generator.
- `AppIcon.icns`: macOS icon family, including 16–1024 pixel representations.
- `MenuBarIconTemplate.pdf`: 18 × 18 point vector with a 1.5 point stroke.

After changing the geometry or colours, run:

```sh
swift scripts/make-icon.swift
./scripts/build-app.sh
```

The generator updates the ICNS, the master PNG, the menu-bar PDF and
`docs/images/icon-preview.png`. Rebuild the app after changing the PDF because
it is embedded at compile time.
