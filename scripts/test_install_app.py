import contextlib
import importlib.util
import io
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("install_app", Path(__file__).with_name("install-app.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallAppTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source = self.root / "build/Winlane.app"
        self.destination = self.root / "Applications/Winlane.app"
        for bundle, text in [(self.source, "new"), (self.destination, "old")]:
            executable = bundle / "Contents/MacOS/winlane"
            executable.parent.mkdir(parents=True)
            executable.write_text(text)
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.output = io.StringIO()
        self.stack.enter_context(contextlib.redirect_stdout(self.output))
        self.verify = self.stack.enter_context(patch.object(installer, "verify_bundle", return_value="1.0"))
        self.identity = self.stack.enter_context(patch.object(installer, "verify_identity"))
        self.quit = self.stack.enter_context(patch.object(installer, "quit_app"))
        self.launch = self.stack.enter_context(patch.object(installer, "launch_app", return_value=[200]))
        self.pids = self.stack.enter_context(patch.object(installer, "installed_pids", return_value=[100]))
        self.preferences = self.stack.enter_context(patch.object(installer, "preferences", return_value={"example": "sensitive-setting-value-123"}))
        self.stack.enter_context(patch.object(installer, "command", side_effect=self.copy_command))

    @staticmethod
    def copy_command(*args, **kwargs):
        if args[0] != "ditto":
            raise AssertionError(f"Unexpected external command: {args}")
        shutil.copytree(args[1], args[2])

    @staticmethod
    def binary(bundle):
        return (bundle / "Contents/MacOS/winlane").read_text()

    def backups(self):
        return list(self.destination.parent.glob(".winlane-update-*/previous/Winlane.app"))

    def test_success_keeps_backup_and_does_not_expose_preferences(self):
        installer.install(self.source, self.destination)
        self.assertEqual(self.binary(self.destination), "new")
        self.assertEqual(len(self.backups()), 1)
        self.assertEqual(self.binary(self.backups()[0]), "old")
        self.assertEqual(self.binary(self.source), "new")
        self.quit.assert_called_once()
        self.launch.assert_called_once()
        self.assertNotIn("sensitive-setting-value-123", self.output.getvalue())
        self.assertIn('"preferences": "unchanged"', self.output.getvalue())

    def test_dry_run_changes_neither_files_nor_processes(self):
        installer.install(self.source, self.destination, dry_run=True)
        self.assertEqual(self.binary(self.destination), "old")
        self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])
        self.quit.assert_not_called()
        self.launch.assert_not_called()

    def test_bad_signature_or_identity_stops_before_quitting(self):
        for check in [self.verify, self.identity]:
            with self.subTest(check=check):
                check.side_effect = RuntimeError("invalid signature")
                with self.assertRaisesRegex(RuntimeError, "invalid signature"):
                    installer.install(self.source, self.destination)
                check.side_effect = None
                self.assertEqual(self.binary(self.destination), "old")
                self.quit.assert_not_called()
                self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])

    def test_failed_quit_leaves_original_installation_untouched(self):
        self.quit.side_effect = RuntimeError("still running")
        with self.assertRaisesRegex(RuntimeError, "still running"):
            installer.install(self.source, self.destination)
        self.assertEqual(self.binary(self.destination), "old")
        self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])
        self.launch.assert_not_called()

    def test_failed_replacement_restores_and_relaunches_previous_app(self):
        rename = Path.rename

        def fail_staged_move(path, target):
            if path.parent.name == "staged":
                raise OSError("replacement failed")
            return rename(path, target)

        with patch.object(Path, "rename", fail_staged_move):
            with self.assertRaisesRegex(RuntimeError, "previous installation restored"):
                installer.install(self.source, self.destination)
        self.assertEqual(self.binary(self.destination), "old")
        self.launch.assert_called_once()
        self.assertEqual(self.backups(), [])

    def test_failed_start_restores_and_relaunches_previous_app(self):
        self.launch.side_effect = [RuntimeError("new app failed"), [101]]
        with self.assertRaisesRegex(RuntimeError, "previous installation restored"):
            installer.install(self.source, self.destination)
        self.assertEqual(self.binary(self.destination), "old")
        self.assertEqual(self.launch.call_count, 2)
        self.assertEqual(self.backups(), [])

    def test_failed_recovery_keeps_the_backup(self):
        self.launch.side_effect = RuntimeError("new app failed")
        self.quit.side_effect = [None, RuntimeError("could not stop new app")]
        with self.assertRaisesRegex(RuntimeError, "Preserved recovery files"):
            installer.install(self.source, self.destination)
        self.assertEqual(self.binary(self.backups()[0]), "old")
        self.assertEqual(self.binary(self.destination), "new")

    def test_changed_preferences_are_reported_without_overwriting_them(self):
        self.preferences.side_effect = [{"a": 1}, {"a": 1}, {"a": 2}]
        with self.assertRaisesRegex(RuntimeError, "saved settings changed"):
            installer.install(self.source, self.destination)
        self.assertEqual(self.binary(self.destination), "new")
        self.assertEqual(self.binary(self.backups()[0]), "old")

    def test_identical_nested_and_symlink_destinations_are_rejected(self):
        nested = self.source / "nested/Winlane.app"
        nested.parent.mkdir()
        symlink = self.root / "linked/Winlane.app"
        symlink.parent.mkdir()
        symlink.symlink_to(self.destination, target_is_directory=True)
        for target in [self.source, nested, symlink]:
            with self.subTest(target=target), self.assertRaises(RuntimeError):
                installer.install(self.source, target)
        self.quit.assert_not_called()

    def test_other_running_copy_stops_before_copying(self):
        with patch.object(installer, "installed_pids", side_effect=RuntimeError("other copy")):
            with self.assertRaisesRegex(RuntimeError, "other copy"):
                installer.install(self.source, self.destination)
        self.quit.assert_not_called()
        self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])


class PlatformChecks(unittest.TestCase):
    def test_requirement_on_stdout_is_used_for_identity_check(self):
        requirement = 'identifier "app.windowlane.desktop" and certificate leaf = H"example"'
        with patch.object(installer, "command") as command:
            command.return_value = subprocess.CompletedProcess([], 0, f"designated => {requirement}\n", "Executable=example")
            installer.verify_identity(Path("build.app"), Path("installed.app"))
            self.assertEqual(command.call_args.args, ("codesign", "--verify", "--strict", "-R", "=" + requirement, "build.app"))

    def test_process_paths_with_spaces_are_preserved(self):
        with patch.object(installer, "command") as command:
            command.return_value = subprocess.CompletedProcess([], 0, " 123 /A Folder/Winlane.app/Contents/MacOS/winlane\n 124 /usr/bin/other\n", "")
            self.assertEqual(installer.processes(), {123: Path("/A Folder/Winlane.app/Contents/MacOS/winlane")})

    def test_other_process_paths_are_rejected(self):
        with patch.object(installer, "processes", return_value={123: Path("/another/Winlane.app/Contents/MacOS/winlane")}):
            with self.assertRaisesRegex(RuntimeError, "other Winlane copies"):
                installer.installed_pids(Path("/Applications/Winlane.app"))

    def test_wrong_bundle_and_adhoc_signature_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            bundle = Path(temp) / "Winlane.app"
            info_path = bundle / "Contents/Info.plist"
            info_path.parent.mkdir(parents=True)
            info_path.write_bytes(plistlib.dumps({"CFBundleIdentifier": "com.example.other"}))
            with patch.object(installer, "command") as command:
                with self.assertRaisesRegex(RuntimeError, "Not a Winlane"):
                    installer.verify_bundle(bundle)
                command.assert_not_called()
                info_path.write_bytes(plistlib.dumps({"CFBundleIdentifier": installer.BUNDLE_ID, "CFBundleExecutable": "winlane"}))
                command.return_value = subprocess.CompletedProcess([], 0, "", "Signature=adhoc\n")
                with self.assertRaisesRegex(RuntimeError, "ad hoc"):
                    installer.verify_bundle(bundle)


if __name__ == "__main__":
    unittest.main()
