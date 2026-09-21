# Offline emoji catalog

`catalog.tsv` contains all 3,781 fully qualified sequences from
[Unicode Emoji 16.0](https://www.unicode.org/Public/emoji/16.0/emoji-test.txt), in
Unicode order. English and Simplified Chinese names and keywords come from
[CLDR JSON 47.0.0](https://github.com/unicode-org/cldr-json/tree/47.0.0), including
derived annotations for flags, skin tones, and joined sequences.

The four tab-separated fields are the complete emoji sequence, English name,
Chinese name, and normalized search terms. Regenerate it with:

```sh
python3 scripts/update-emoji-data.py
```

Only regeneration requires network access. Rust's `include_str!` embeds the
catalog into the executable, so release builds work offline without these source
files. Names and search terms are borrowed from the embedded data; the catalog
does not load image files. AppKit renders the characters with macOS fonts. Older
macOS releases may not render emoji introduced by newer Unicode versions.

The data is covered by the [Unicode License v3](LICENSE.txt). The build script
also includes this license in the application bundle as `Unicode-LICENSE.txt`.
