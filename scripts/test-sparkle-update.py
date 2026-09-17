#!/usr/bin/env python3
"""Exercise Sparkle with disposable bundles, a loopback feed, and ephemeral keys."""

import functools
import http.server
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid


ROOT = Path(__file__).resolve().parent.parent


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def test(bundle, directory, mode):
    identity = "app.windowlane.sparkle-test." + str(uuid.uuid4())
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(QuietHandler, directory=str(directory))
    )
    threading.Thread(target=server.serve_forever, daemon=True).start()
    prefix = f"http://127.0.0.1:{server.server_port}/"
    marker = directory / "relaunched"
    sparkle = Path(subprocess.check_output([ROOT / "scripts/fetch-sparkle.sh"], text=True).strip())
    try:
        run("swift", "-e", """
import CryptoKit
import Foundation
let key = Curve25519.Signing.PrivateKey()
let directory = URL(fileURLWithPath: CommandLine.arguments[1])
try key.rawRepresentation.base64EncodedString().write(to: directory.appendingPathComponent("key"), atomically: true, encoding: .utf8)
try key.publicKey.rawRepresentation.base64EncodedString().write(to: directory.appendingPathComponent("public"), atomically: true, encoding: .utf8)
""", str(directory))
        binary = directory / "host"
        run("clang", "-fobjc-arc", "-mmacosx-version-min=14.0", "-framework", "AppKit",
            "-F", str(sparkle), "-framework", "Sparkle",
            "-Wl,-rpath,@executable_path/../Frameworks",
            str(ROOT / "tests/sparkle_host.m"), "-o", str(binary))
        host = directory / "installed" / "Probe.app"
        update = directory / "new" / "Probe.app"
        for app, version in [(host, "1.0.0"), (update, "1.0.1")]:
            app.parent.mkdir()
            run("ditto", str(bundle), str(app))
            shutil.copy2(binary, app / "Contents/MacOS/winlane")
            path = app / "Contents/Info.plist"
            info = plistlib.loads(path.read_bytes())
            info.update({
                "CFBundleIdentifier": identity, "CFBundleName": "Winlane Update Test",
                "CFBundleDisplayName": "Winlane Update Test",
                "CFBundleVersion": version, "CFBundleShortVersionString": version,
                "SUFeedURL": prefix + "appcast.xml", "SUEnableAutomaticChecks": False,
                "SUPublicEDKey": (directory / "public").read_text(),
                "TestMarker": str(marker),
                "NSAppTransportSecurity": {"NSAllowsLocalNetworking": True},
            })
            if mode == "tampered-archive":
                info["TestExpectedError"] = 4005
            elif mode == "tampered-feed":
                info["TestExpectedError"] = 1000
            path.write_bytes(plistlib.dumps(info))
            run("codesign", "--force", "--sign", os.environ.get("WINLANE_TEST_SIGNING_IDENTITY", "-"),
                "--timestamp=none", "--identifier", identity, str(app))
        archive = directory / "update.zip"
        run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(update), str(archive))
        run(str(sparkle / "bin/generate_appcast"), "--ed-key-file", str(directory / "key"),
            "--maximum-deltas", "0", "--download-url-prefix", prefix, str(directory))
        if mode == "tampered-archive":
            content = bytearray(archive.read_bytes())
            content[len(content) // 2] ^= 1
            archive.write_bytes(content)
        elif mode == "tampered-feed":
            feed = directory / "appcast.xml"
            feed.write_bytes(feed.read_bytes().replace(b"<title>1.0.1</title>", b"<title>9.9.9</title>"))
        with (directory / "host.log").open("w") as log:
            process = subprocess.Popen([str(host / "Contents/MacOS/winlane")], stdout=log, stderr=log)
            try:
                result = process.wait(timeout=55)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
                raise
        if result != 0:
            raise RuntimeError(f"{mode} failed (exit {result}):\n" + (directory / "host.log").read_text())
        if mode == "valid":
            deadline = time.monotonic() + 15
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.1)
            assert marker.read_text() == "updated and relaunched", "new version did not relaunch"
            assert plistlib.loads((host / "Contents/Info.plist").read_bytes())["CFBundleVersion"] == "1.0.1"
            run("codesign", "--verify", "--deep", "--strict", str(host))
        else:
            assert not marker.exists(), "a tampered update was installed"
            assert plistlib.loads((host / "Contents/Info.plist").read_bytes())["CFBundleVersion"] == "1.0.0"
        print(f"Sparkle end-to-end test passed: {mode}", flush=True)
    finally:
        server.shutdown()
        server.server_close()
        subprocess.run(["defaults", "delete", identity], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        shutil.rmtree(Path.home() / "Library/Caches" / identity, ignore_errors=True)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: test-sparkle-update.py APP_BUNDLE")
    for case in ("valid", "tampered-archive", "tampered-feed"):
        with tempfile.TemporaryDirectory(prefix="winlane-sparkle-test-") as temporary:
            test(Path(sys.argv[1]).resolve(), Path(temporary), case)
