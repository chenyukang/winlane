#!/usr/bin/env python3
"""Regenerate the offline emoji catalog from pinned Unicode releases."""

import json
from pathlib import Path
import re
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
CLDR = "https://raw.githubusercontent.com/unicode-org/cldr-json/47.0.0/cldr-json"
EMOJI = "https://www.unicode.org/Public/emoji/16.0/emoji-test.txt"


def fetch(url):
    with urlopen(url, timeout=60) as response:
        return response.read().decode("utf-8")


def key(text):
    return text.replace("\ufe0f", "")


def normalize(text):
    return " ".join(re.sub(r"[_:\-]", " ", text.lower()).split())


def annotations(language):
    result = {}
    for kind in ("annotations", "annotations-derived"):
        directory = "annotationsDerived" if kind == "annotations-derived" else kind
        data = json.loads(fetch(f"{CLDR}/cldr-{kind}-full/{directory}/{language}/annotations.json"))
        result.update({key(glyph): value for glyph, value in data[directory]["annotations"].items()})
    return result


def generate(source, english, chinese):
    rows = []
    for line in source.splitlines():
        match = re.match(r"^([0-9A-F ]+); fully-qualified\s+# \S+ E[\d.]+ (.+)$", line)
        if not match:
            continue
        glyph = "".join(chr(int(code, 16)) for code in match[1].split())
        en = english.get(key(glyph), {})
        zh = chinese.get(key(glyph), {})
        name = en.get("tts", [match[2]])[0]
        translated = zh.get("tts", [name])[0]
        keywords = list(dict.fromkeys(normalize(word) for word in
                        [name, translated, *en.get("default", []), *zh.get("default", [])]))
        rows.append("\t".join((glyph, name, translated, " | ".join(keywords))))
    return "\n".join(rows) + "\n"


def main():
    output = generate(fetch(EMOJI), annotations("en"), annotations("zh"))
    if len(output.splitlines()) != 3781:
        raise ValueError("Unexpected Unicode 16.0 emoji count; catalog not replaced")
    destination = ROOT / "resources" / "emoji"
    destination.mkdir(exist_ok=True)
    (destination / "catalog.tsv").write_text(output, encoding="utf-8")
    license_text = fetch("https://raw.githubusercontent.com/unicode-org/cldr-json/47.0.0/LICENSE")
    (destination / "LICENSE.txt").write_text(license_text, encoding="utf-8")
    print(f"Wrote {len(output.splitlines())} emoji to {destination}")


if __name__ == "__main__":
    main()
