#!/usr/bin/env python3
"""Install a signed local Winlane build, preserving settings and the previous app."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time

BUNDLE_ID = "app.windowlane.desktop"
PREFERENCES_KEY = "WindowlanePreferencesV1"
DEFAULT_SOURCE = Path(__file__).resolve().parent.parent / "dist/Winlane.app"
DEFAULT_DESTINATION = Path("/Applications/Winlane.app")


def command(*args, timeout=30):
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed: {(result.stderr or result.stdout).strip()}")
    return result


def verify_bundle(path):
    with (path / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info.get("CFBundleIdentifier") != BUNDLE_ID or info.get("CFBundleExecutable") != "winlane":
        raise RuntimeError(f"Not a Winlane application bundle: {path}")
    command("codesign", "--verify", "--deep", "--strict", str(path))
    signature = command("codesign", "-d", "--verbose=2", str(path))
    if "Signature=adhoc" in signature.stderr:
        raise RuntimeError("Refusing an ad hoc installation; build with the existing signing identity.")
    return info.get("CFBundleShortVersionString", "unknown")


def verify_identity(source, installed):
    signature = command("codesign", "-d", "-r-", str(installed))
    requirement = next(
        (line.removeprefix("designated => ") for line in (signature.stdout + signature.stderr).splitlines()
         if line.startswith("designated => ")), None
    )
    if not requirement:
        raise RuntimeError("Could not read the installed app's signing requirement.")
    command("codesign", "--verify", "--strict", "-R", "=" + requirement, str(source))


def processes():
    found = {}
    for line in command("ps", "-axo", "pid=,comm=").stdout.splitlines():
        fields = line.strip().split(None, 1)
        if len(fields) == 2 and Path(fields[1]).name == "winlane":
            found[int(fields[0])] = Path(fields[1]).resolve()
    return found


def installed_pids(destination):
    executable = destination / "Contents/MacOS/winlane"
    running = processes()
    other = {pid: str(path) for pid, path in running.items() if path != executable}
    if other:
        raise RuntimeError(f"Quit other Winlane copies before installing: {other}")
    return sorted(running)


def preferences():
    result = subprocess.run(
        ["defaults", "read", BUNDLE_ID, PREFERENCES_KEY], capture_output=True, text=True, timeout=10
    )
    if result.returncode:
        if "does not exist" in result.stderr:
            return None
        raise RuntimeError("Could not read saved Winlane settings; installation was stopped.")
    # Compare settings without printing their potentially private contents.
    return json.loads(result.stdout)


def quit_app(destination, timeout):
    old_pids = installed_pids(destination)
    if not old_pids:
        return
    command(
        "osascript", "-e", f"with timeout of {math.ceil(timeout)} seconds",
        "-e", f'tell application id "{BUNDLE_ID}" to quit', "-e", "end timeout",
        timeout=timeout + 2,
    )
    deadline = time.monotonic() + timeout
    while installed_pids(destination):
        if time.monotonic() >= deadline:
            raise RuntimeError("Winlane did not exit normally; no force quit was attempted.")
        time.sleep(0.2)


def launch_app(destination, timeout):
    command("open", str(destination), timeout=timeout)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        pids = installed_pids(destination)
        if pids:
            time.sleep(2)
            if set(pids) & set(installed_pids(destination)):
                return pids
        time.sleep(0.2)
    raise RuntimeError("Winlane did not remain running after launch.")


def executable_hash(bundle):
    with (bundle / "Contents/MacOS/winlane").open("rb") as stream:
        digest = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
        return digest.hexdigest()


def install(source, destination, timeout=10, dry_run=False):
    source = source.expanduser().resolve()
    destination = destination.expanduser().absolute()
    if destination.is_symlink():
        raise RuntimeError("The installation destination must not be a symbolic link.")
    destination = destination.resolve()
    if destination.name != "Winlane.app" or not destination.parent.is_dir():
        raise RuntimeError("Choose an existing installation directory with a Winlane.app destination.")
    if source == destination or source in destination.parents or destination in source.parents:
        raise RuntimeError("The build and installed application must be separate, non-nested paths.")
    version = verify_bundle(source)
    had_installed = destination.exists()
    if had_installed:
        verify_bundle(destination)
        verify_identity(source, destination)
    old_pids = installed_pids(destination)
    before = preferences()
    expected_hash = executable_hash(source)
    print(f"Verified Winlane {version}: {source} -> {destination}", flush=True)
    if dry_run:
        print(f"Dry run: existing PIDs {old_pids}; no files or running apps changed.")
        return

    work = Path(tempfile.mkdtemp(prefix=".winlane-update-", dir=destination.parent))
    staged = work / "staged/Winlane.app"
    backup = work / "previous/Winlane.app"
    backup.parent.mkdir()
    replaced = False
    keep_work = False
    try:
        command("ditto", str(source), str(staged), timeout=120)
        verify_bundle(staged)
        if destination.exists():
            verify_identity(staged, destination)
        if executable_hash(staged) != expected_hash:
            raise RuntimeError("The staged executable differs from the build.")
        print("Quitting Winlane normally…", flush=True)
        quit_app(destination, timeout)
        try:
            # Finish any editor autosave on quit before checking restart preservation.
            before = preferences()
            if destination.exists():
                destination.rename(backup)
            staged.rename(destination)
            replaced = True
            verify_bundle(destination)
            if executable_hash(destination) != expected_hash:
                raise RuntimeError("The installed executable differs from the build.")
            new_pids = launch_app(destination, timeout)
            if set(new_pids) & set(old_pids):
                raise RuntimeError("The original Winlane process is still running.")
        except Exception as error:
            try:
                if replaced:
                    quit_app(destination, timeout)
                    destination.rename(work / "failed.app")
                if backup.exists():
                    backup.rename(destination)
                if old_pids and destination.exists():
                    launch_app(destination, timeout)
            except Exception as rollback_error:
                keep_work = True
                raise RuntimeError(
                    f"Installation failed: {error}. Recovery also failed: {rollback_error}. "
                    f"Preserved recovery files: {work}"
                ) from error
            recovery = "previous installation restored" if had_installed else "new installation removed"
            raise RuntimeError(f"Installation failed; {recovery}: {error}") from error
        if preferences() != before:
            raise RuntimeError(
                f"Winlane restarted, but saved settings changed. Settings were not overwritten. "
                f"Inspect them before continuing. Previous app: {backup if backup.exists() else 'none'}"
            )
        print(json.dumps({
            "installed": str(destination), "previous_pids": old_pids, "pids": new_pids,
            "signature": "verified", "executable": "matches build", "preferences": "unchanged",
            "backup": str(backup) if backup.exists() else None,
        }, indent=2))
    finally:
        # Keep the only backup if recovery cannot complete, as well as successful backups.
        if not backup.exists() and not keep_work:
            shutil.rmtree(work)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", nargs="?", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--destination", type=Path, default=DEFAULT_DESTINATION)
    parser.add_argument("--timeout", type=float, default=10, help="Quit/start timeout in seconds (default: 10)")
    parser.add_argument("--dry-run", action="store_true", help="Verify the build, identity and running copies without installing")
    args = parser.parse_args()
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be a positive finite number")
    if sys.platform != "darwin":
        parser.error("Installing Winlane requires macOS")
    try:
        install(args.bundle, args.destination, args.timeout, args.dry_run)
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"Installation stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
