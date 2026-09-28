# Key remaps

Winlane already watches every keystroke to run its own shortcuts, so it can also rewrite a combination before any application sees it — the same job a keyboard remapper does. Rules stay plain: one key plus the modifiers held with it becomes another key plus its modifiers, optionally skipping named applications.

## Editing rules

**Settings → Shortcuts → Key remaps** lists every rule with a checkbox, its two combinations and its excluded applications. **Enable key remaps** at the top is a master switch: turning it off suspends every rule at once while leaving them in the list. Untick a rule to switch it off without losing it, **Delete** removes it, and **＋ Add Rule** appends one. Changes save automatically, and a rule that cannot be read is refused with the reason instead of replacing the active rules.

A combination is written the way it reads: modifiers as `⌃⌥⇧⌘` or as names (`control`, `cmd`, `shift`, `alt`) followed by a key, for example `⌃S`, `⌘⇧K`, `f7`, `left_arrow`, `page_up`, `delete_or_backspace`. A new rule starts disabled so that adding one is always valid while it is being filled in.

**Excluded applications** are edited as applications, not as identifiers: the rule's list expands to show each one with its icon and name, **＋ Add App…** opens the same application panel the input rules use, and **Exclude terminals** or **Exclude remote desktops** adds the lists a migrated Karabiner rule usually carries. An entry that ends with a dot covers every bundle under that prefix, which is how a family such as `com.parallels.winapp.` is written.

## What a rule is

Rules live in Winlane's saved preferences under `key_remaps`:

| Field | Meaning |
| --- | --- |
| `id` | Stable name for the rule. Two enabled rules may not match the same combination. |
| `enabled` | Off keeps the rule in place without applying it. Defaults to on. |
| `from_key` | The key to match, spelled the way Karabiner spells it: `s`, `f7`, `page_up`, `end`, `delete_or_backspace`, `left_arrow`. |
| `from_modifiers` | Modifiers that must be held: `control`, `left_command`, `option`, `shift`. Empty matches the bare key. Either side of a modifier counts, so `left_control` and `control` behave alike. |
| `allow_extra_modifiers` | Also fire when other modifiers are held. Off means the listed modifiers have to be the whole set, so a rule for a bare key does not steal `Shift` plus that key. |
| `to_key` | The key to send instead. |
| `to_modifiers` | The complete modifier set the event carries: the modifiers held on the way in are replaced, not merged. |
| `except_apps` | Bundle identifiers to leave alone, such as `com.apple.Terminal`; one ending in a dot covers that prefix. |

A rule is applied on the press and the release is rewritten the same way, so the combination arrives as a whole even if a modifier is released first. Auto-repeat follows the press as well.

## Migrating from Karabiner-Elements

```sh
python3 scripts/migrate-karabiner.py > key_remaps.json
```

The script converts every plain remap it understands and reports the rest. Layers, shell commands, mouse keys and held-key behaviour are listed as skipped instead of being guessed at, so a migration never quietly changes what a key does.

## Behaviour worth knowing

- **Winlane's own shortcuts win.** A combination Winlane is configured to handle — the search or switch shortcut, or an app, quicklink or command shortcut — is never remapped, so a migrated rule cannot silently shadow a shortcut you set up in Winlane. Every other combination is remapped in every application, Winlane's panel included.
- **Exclusions need to know the frontmost application.** Winlane publishes it whenever an application is activated. Until it knows, a rule that names applications stays off rather than firing where it should not.
- **This is an event tap, not a keyboard device.** Karabiner-Elements replaces the keyboard at the HID layer; Winlane rewrites events after the system has already seen them. Remaps therefore do not apply during secure input (password fields, Terminal's Secure Keyboard Entry), and they stop while Winlane is not running or whenever macOS disables the tap.
- **Failure is inert.** A rule that cannot be parsed is dropped, and an invalid rule is rejected before it replaces the active configuration, so a bad edit leaves the previous rules running.
- **Winlane's own defaults are empty.** No rule ships with the app; the migration script and the settings page only ever change your saved configuration.

## Not supported

Layers, chords, tap-versus-hold (dual-role keys), mouse buttons, per-device rules and shell commands. Those need timing state that a keystroke rewriter cannot express safely, which is why they are reported instead of half-implemented.
