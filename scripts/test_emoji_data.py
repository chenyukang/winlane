import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "emoji_data", Path(__file__).with_name("update-emoji-data.py")
)
emoji_data = importlib.util.module_from_spec(spec)
spec.loader.exec_module(emoji_data)


class EmojiDataTests(unittest.TestCase):
    def test_qualified_sequences_keep_selectors_and_keyword_languages(self):
        source = """# group: test
2764 FE0F ; fully-qualified # ❤️ E0.6 red heart
2764 ; unqualified # ❤ E0.6 red heart
1F469 1F3FD 200D 1F4BB ; fully-qualified # 👩🏽‍💻 E4.0 woman technologist: medium skin tone
"""
        english = {"❤": {"tts": ["red heart"], "default": ["LOVE", "heart"]}}
        chinese = {"❤": {"tts": ["红心"], "default": ["爱", "心"]}}
        rows = emoji_data.generate(source, english, chinese).splitlines()
        self.assertEqual(len(rows), 2)
        fields = rows[0].split("\t")
        self.assertEqual(fields[:3], ["❤️", "red heart", "红心"])
        self.assertIn("love", fields[3])
        self.assertIn("爱", fields[3])
        self.assertEqual(rows[1].split("\t")[0], "👩🏽‍💻")


if __name__ == "__main__":
    unittest.main()
