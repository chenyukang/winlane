import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "migrate_karabiner", Path(__file__).with_name("migrate-karabiner.py")
)
migrate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migrate)


def document(*manipulators, description="A rule", conditions=None):
    return {
        "profiles": [
            {
                "complex_modifications": {
                    "rules": [
                        {
                            "description": description,
                            "manipulators": list(manipulators),
                        }
                    ]
                }
            }
        ]
    }


class MigrateKarabinerTests(unittest.TestCase):
    def test_plain_remap_becomes_one_rule_per_manipulator(self):
        rules, skipped = migrate.convert(
            document(
                {
                    "from": {"key_code": "s", "modifiers": {"mandatory": ["left_control"], "optional": ["any"]}},
                    "to": [{"key_code": "s", "modifiers": ["left_command"]}],
                },
                {
                    "from": {"key_code": "l", "modifiers": {"mandatory": ["left_command"], "optional": ["any"]}},
                    "to": [{"key_code": "right_arrow"}],
                },
                description="Ctrl+S => Cmd+S (Save)",
            )
        )
        self.assertEqual(skipped, [])
        self.assertEqual(len(rules), 2)
        save = rules[0]
        self.assertEqual(save["id"], "ctrl-s-cmd-s-save-1")
        self.assertEqual(save["from_key"], "s")
        self.assertEqual(save["from_modifiers"], ["left_control"])
        self.assertTrue(save["allow_extra_modifiers"])
        self.assertEqual(save["to_key"], "s")
        self.assertEqual(save["to_modifiers"], ["left_command"])
        self.assertTrue(save["enabled"])
        arrow = rules[1]
        self.assertEqual(arrow["id"], "ctrl-s-cmd-s-save-2")
        self.assertEqual(arrow["to_key"], "right_arrow")
        self.assertNotIn("to_modifiers", arrow)

    def test_application_exclusions_lose_their_anchors(self):
        rules, skipped = migrate.convert(
            document(
                {
                    "from": {"key_code": "w", "modifiers": {"mandatory": ["control"], "optional": ["any"]}},
                    "to": [{"key_code": "w", "modifiers": ["command"]}],
                    "conditions": [
                        {
                            "type": "frontmost_application_unless",
                            "bundle_identifiers": ["^com\\.apple\\.Terminal$", "^net\\.kovidgoyal\\.kitty$"],
                        }
                    ],
                }
            )
        )
        self.assertEqual(skipped, [])
        self.assertEqual(
            rules[0]["except_apps"], ["com.apple.Terminal", "net.kovidgoyal.kitty"]
        )

    def test_rules_without_a_plain_key_are_reported_not_guessed(self):
        cases = [
            ({"from": {"key_code": "f17"}, "to_if_alone": [{"key_code": "left_shift"}]}, "to_if_alone"),
            ({"from": {"key_code": "f17"}}, "exactly one key"),
            ({"from": {"key_code": "a"}, "to": [{"shell_command": "open -a X"}]}, "one plain key"),
            (
                {
                    "from": {"key_code": "a"},
                    "to": [{"key_code": "b"}],
                    "conditions": [{"type": "frontmost_application_if", "bundle_identifiers": ["^x$"]}],
                },
                "only inside one application",
            ),
            (
                {
                    "from": {"key_code": "a", "modifiers": {"mandatory": ["control"], "optional": ["caps_lock"]}},
                    "to": [{"key_code": "b"}],
                },
                "optional modifiers",
            ),
        ]
        for manipulator, expected in cases:
            rules, skipped = migrate.convert(document(manipulator))
            self.assertEqual(rules, [], manipulator)
            self.assertEqual(len(skipped), 1)
            self.assertIn(expected, skipped[0])

    def test_ids_stay_unique_across_profiles(self):
        payload = document(
            {
                "from": {"key_code": "a"},
                "to": [{"key_code": "b"}],
            },
            description="Same description",
        )
        payload["profiles"].append(payload["profiles"][0])
        rules, _ = migrate.convert(payload)
        self.assertEqual(len(rules), 2)
        self.assertEqual(len({rule["id"] for rule in rules}), 2)


if __name__ == "__main__":
    unittest.main()
