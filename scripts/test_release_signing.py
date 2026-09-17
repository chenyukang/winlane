import itertools
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from release_signing import signing_mode


class ReleaseSigningTests(unittest.TestCase):
    def test_complete_credentials_select_the_expected_mode(self):
        certificate = dict.fromkeys(("CERT_P12", "CERT_PASSWORD", "SIGN_IDENTITY"), "set")
        notary = dict.fromkeys(("NOTARY_KEY", "NOTARY_KEY_ID", "NOTARY_ISSUER"), "set")
        self.assertEqual(signing_mode({}), "ad-hoc")
        self.assertEqual(signing_mode(certificate), "certificate")
        self.assertEqual(signing_mode(certificate | notary), "notarized")
        self.assertEqual(signing_mode(certificate | {"REQUIRE_SIGNING": "true"}), "certificate")
        self.assertEqual(signing_mode(certificate | notary | {"REQUIRE_NOTARIZATION": "true"}), "notarized")

    def test_partial_credentials_never_fall_back(self):
        keys = ("CERT_P12", "CERT_PASSWORD", "SIGN_IDENTITY", "NOTARY_KEY", "NOTARY_KEY_ID", "NOTARY_ISSUER")
        valid = {(False,) * 6, (True,) * 3 + (False,) * 3, (True,) * 6}
        for bits in itertools.product((False, True), repeat=6):
            if bits in valid:
                continue
            with self.subTest(bits=bits), self.assertRaises(ValueError):
                signing_mode({key: "set" for key, present in zip(keys, bits) if present})

    def test_required_modes_fail_closed(self):
        for key in ("REQUIRE_SIGNING", "REQUIRE_NOTARIZATION"):
            with self.subTest(key=key), self.assertRaises(ValueError):
                signing_mode({key: "true"})
            with self.subTest(key=key, invalid=True), self.assertRaises(ValueError):
                signing_mode({key: "typo"})
        with self.assertRaises(ValueError):
            signing_mode({"CERT_P12": "set", "CERT_PASSWORD": "set", "SIGN_IDENTITY": "set", "REQUIRE_NOTARIZATION": "true"})

    def test_failure_does_not_write_a_mode_or_expose_credentials(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            secret = "test-secret-that-must-not-be-logged"
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("release_signing.py"))],
                env={"PATH": os.defpath, "GITHUB_OUTPUT": str(output), "CERT_P12": secret},
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertNotIn(secret, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
