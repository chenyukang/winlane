# Winlane icons

The app icon uses two overlapping window frames in porcelain white and translucent lavender, on a blue-to-periwinkle tile. The menu-bar mark uses the same silhouette as a monochrome vector.

- `AppIcon.png`: master artwork, generated with the built-in ImageGen tool; transparent outside the tile.
- `AppIcon.icns`: macOS icon family, including 16–1024 pixel representations.
- `MenuBarIconTemplate.pdf`: 18 × 18 point vector with a 1.5 point stroke. Embedded in the executable and rendered as an AppKit template image, so macOS supplies the appropriate foreground color.

After replacing the master artwork or changing the menu-bar geometry, run:

```sh
swift scripts/make-icon.swift
./scripts/build-app.sh
```

The generator updates the ICNS, menu-bar PDF, and `docs/images/icon-preview.png`. Rebuild the app after changing the PDF because it is embedded at compile time. The PNG is the source artwork; the script does not invoke an image-generation service.

## Image-generation prompt

Design the FINAL production macOS application icon for Winlane, a refined native window switcher for developers. Output one single 1024x1024 RGBA PNG icon, real transparent background outside the icon, no presentation board, no words, no letters typeset, no labels or watermarks. A carefully proportioned macOS rounded-square tile occupying about 86% of the canvas, perfectly centered and straight-on. Rich saturated ultramarine blue at the lower left blending into luminous periwinkle at the upper right, premium smooth softly lit enamel/glass finish, restrained bevel and very soft tight shadow. Inside: a bold beautiful symbol of TWO overlapping upright macOS window frames stepping diagonally from back upper-left to front lower-right. The rear window is translucent pale lavender glass; the front window is luminous porcelain-white rim with a rich blue open interior. Both have generous rounded corners, strong broad frame weight, and a single horizontal titlebar divider near the top; no traffic-light dots, no UI text or tiny details. The back window is offset by about one fifth of its width left and upward, partly occluded by the front frame. Elegant abstract architecture, spacious icon with one decisive clear silhouette, dimensional depth only through restrained material shading. The combined symbol occupies 62 percent of the tile width and 58 percent of the tile height, optically centered. Crisp consistent geometry, premium independent Mac productivity application identity, readable at 32 pixels. Do not put the tile on a white background; preserve true transparent alpha outside its rounded corners.
