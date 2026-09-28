#!/usr/bin/env python3
"""Convert Karabiner-Elements complex modifications into Winlane key remaps.

Only the part both tools share is converted: one key plus its modifiers
becomes another key plus its modifiers, optionally skipping some applications.
Anything else - layers, shell commands, mouse keys, held-key behaviour - is
reported and left out, so a migration never quietly changes what a key does.

Usage:
    python3 scripts/migrate-karabiner.py [karabiner.json]

The converted rules are printed as a JSON array, ready to be placed under
"key_remaps" in Winlane's saved preferences.
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

DEFAULT_CONFIG = pathlib.Path.home() / ".config/karabiner/karabiner.json"

# Karabiner names that Winlane spells differently.
KEY_ALIASES = {
    "spacebar": "spacebar",
    "delete_or_backspace": "delete_or_backspace",
    "return_or_enter": "return_or_enter",
}


def modifier_name(name: str) -> str:
    """Winlane accepts Karabiner's side-specific spelling; keep it as written."""
    return name


def bundle_id(pattern: str) -> str:
    """`^com\\.apple\\.Terminal$` in Karabiner is a plain bundle id here."""
    # Karabiner writes these as regular expressions; strip the anchors and the
    # escaping so the result is the bundle identifier itself.
    return re.sub(r"^\^|\$$", "", pattern).replace("\\", "")


def convert_manipulator(manipulator: dict) -> tuple[dict | None, str | None]:
    """Return the Winlane rule and the application exclusions it needs."""
    unsupported = [
        key
        for key in (
            "to_if_alone",
            "to_if_held_down",
            "to_after_key_up",
            "to_delayed_action",
            "set_variable",
            "shell_command",
            "mouse_key",
        )
        if key in manipulator
    ]
    if unsupported:
        return None, "uses " + ", ".join(unsupported)

    source = manipulator.get("from")
    if not isinstance(source, dict) or "key_code" not in source:
        return None, "has no single physical key to match"

    target = manipulator.get("to")
    if not isinstance(target, list) or len(target) != 1:
        return None, "does not send exactly one key"
    event = target[0]
    if "key_code" not in event or event.get("lazy"):
        return None, "does not send one plain key press"

    modifiers = source.get("modifiers", {})
    optional = modifiers.get("optional", []) or []
    if any(value not in ("any",) for value in optional):
        return None, "lists optional modifiers that are not 'any'"

    exclusions: list[str] = []
    for condition in manipulator.get("conditions", []) or []:
        kind = condition.get("type")
        if kind == "frontmost_application_unless":
            exclusions.extend(bundle_id(value) for value in condition.get("bundle_identifiers", []))
        elif kind == "frontmost_application_if":
            return None, "applies only inside one application"
        else:
            return None, f"uses the condition {kind}"

    # Empty lists are left out, so the emitted JSON matches what Winlane writes
    # back once the rules are saved from the app.
    rule = {"id": "", "enabled": True, "from_key": source["key_code"]}
    from_modifiers = [modifier_name(value) for value in modifiers.get("mandatory", [])]
    if from_modifiers:
        rule["from_modifiers"] = from_modifiers
    if "any" in optional:
        rule["allow_extra_modifiers"] = True
    rule["to_key"] = event["key_code"]
    to_modifiers = [modifier_name(value) for value in event.get("modifiers", [])]
    if to_modifiers:
        rule["to_modifiers"] = to_modifiers
    if exclusions:
        rule["except_apps"] = sorted(set(exclusions))
    return rule, None


def slug(text: str) -> str:
    cleaned = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return cleaned or "rule"


def convert(document: dict) -> tuple[list[dict], list[str]]:
    """Convert every profile, skipping rules that are not a plain remap."""
    rules: list[dict] = []
    skipped: list[str] = []
    for profile in document.get("profiles", []):
        for rule in profile.get("complex_modifications", {}).get("rules", []):
            description = rule.get("description") or "untitled rule"
            for manipulator in rule.get("manipulators", []):
                converted, reason = convert_manipulator(manipulator)
                if converted is None:
                    skipped.append(f"{description}: {reason}")
                    continue
                # Numbered across the whole document so ids stay unique
                # even when two profiles share a rule description.
                converted["id"] = f"{slug(description)}-{len(rules) + 1}"
                rules.append(converted)
    return rules, skipped


def main(argv: list[str]) -> int:
    path = pathlib.Path(argv[1]).expanduser() if len(argv) > 1 else DEFAULT_CONFIG
    try:
        document = json.loads(path.read_text())
    except OSError as error:
        print(f"cannot read {path}: {error}", file=sys.stderr)
        return 1
    except json.JSONDecodeError as error:
        print(f"{path} is not valid JSON: {error}", file=sys.stderr)
        return 1

    rules, skipped = convert(document)
    print(json.dumps(rules, indent=2, ensure_ascii=False))
    if skipped:
        print("", file=sys.stderr)
        print(f"skipped {len(skipped)} manipulator(s):", file=sys.stderr)
        for reason in skipped:
            print(f"  - {reason}", file=sys.stderr)
    print(f"converted {len(rules)} rule(s) from {path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
