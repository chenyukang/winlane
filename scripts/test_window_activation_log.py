import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import contextlib
import io
import subprocess

spec = importlib.util.spec_from_file_location("activation_log", Path(__file__).with_name("check-window-activation-log.py"))
log = importlib.util.module_from_spec(spec)
spec.loader.exec_module(log)


def event(name, elapsed, **extra):
    return {"process_pid": 1, "event": name, "dropped": 0, "data": {
        "request_id": 10, "target_pid": 20, "window_id": 7, "server_id": 70,
        "bundle_id": "example.app", "elapsed_ms": elapsed,
        "front_pid": 20, "active": True, "running": True, "focused_id": 7,
        "on_screen": True, "alive": True, **extra}}


def success():
    return [event("requested", 0), event("cooperative-result", 1, allowed=True),
            event("observed", 20), event("confirmed", 80, stable_ms=60)]


class ActivationLogTests(unittest.TestCase):
    def test_actual_focus_evidence_passes_but_acceptance_alone_does_not(self):
        self.assertEqual(log.verify(success())[0], 0)
        self.assertEqual(log.verify(success()[:2])[0], 2)
        for changes in [{"front_pid": 30}, {"active": False}, {"focused_id": 8},
                        {"on_screen": False}, {"alive": False}, {"stable_ms": 0}]:
            chain = success()
            chain[-1]["data"].update(changes)
            self.assertEqual(log.verify(chain)[0], 1)

    def test_rebuilt_identity_and_unknown_visibility_are_reported_honestly(self):
        chain = success()
        for item in chain:
            item["data"].update(focused_id=999, focused_server_id=70, on_screen=None)
        code, report = log.verify(chain)
        self.assertEqual(code, 0)
        self.assertFalse(report["visibility_verified"])

    def test_failure_cancellation_missing_observation_and_stale_actions(self):
        self.assertEqual(log.verify([event("requested", 0), event("failed", 1500, reason="timeout")])[0], 1)
        chain = [event("requested", 0), event("cancelled", 100, reason="new-gesture"), event("late-completion", 200, ignored=True)]
        self.assertEqual(log.verify(chain)[0], 3)
        chain.append(event("raise-requested", 300))
        self.assertEqual(log.verify(chain)[0], 1)
        self.assertEqual(log.verify([success()[0], success()[-1]])[0], 2)
        self.assertEqual(log.verify([success()[0], *success()])[0], 1)

    def test_requests_are_scoped_to_processes_and_bundle_and_dropped_logs_are_incomplete(self):
        chain = success()
        newer = [event("requested", 0), event("cancelled", 20)]
        for item in newer:
            item["process_pid"] = 2
        self.assertEqual(log.verify(chain + newer)[0], 3)
        self.assertEqual(log.verify(chain, bundle="other.app")[0], 2)
        chain[1]["dropped"] = 1
        self.assertEqual(log.verify(chain)[0], 2)

    def test_old_process_success_is_not_current_process_evidence(self):
        chain = success()
        self.assertEqual(log.verify(chain, process_pid=2)[0], 2)
        chain.append({"process_pid":2,"event":"process-start","dropped":0,"data":{}})
        self.assertEqual(log.verify(chain)[0], 2)
        self.assertEqual(log.verify(chain, request="10", process_pid=1)[0], 0)

    def test_cli_defaults_to_the_running_process_and_history_is_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "activation.log"
            path.write_text("\n".join(
                '2026-10-20T00:00:00.000+08:00 Info pid=1 component="window-activation" '
                f'event="{item["event"]}" dropped=0 data={json.dumps(json.dumps(item["data"]))}'
                for item in success()))
            argv = ["check", "--log", str(path), "--bundle-id", "example.app"]
            with patch("sys.argv", argv), patch.object(log.subprocess,"run",return_value=subprocess.CompletedProcess([],0,"2\n","")), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(log.main(), 2)
            with patch("sys.argv", argv + ["--request-id","10"]), patch.object(log.subprocess,"run") as process, contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(log.main(), 0)
                process.assert_not_called()

    def test_parser_handles_nested_json_without_app_content(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "activation.log"
            path.write_text("\n".join(
                '2026-10-20T00:00:00.000+08:00 Info pid=1 component="window-activation" '
                f'event="{item["event"]}" dropped=0 data={json.dumps(json.dumps(item["data"]))}'
                for item in success()))
            self.assertEqual(log.verify(log.read_events([path]))[0], 0)


if __name__ == "__main__":
    unittest.main()
