#!/usr/bin/env python3
"""Verify evidence for the latest window activation; never controls applications."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

COMPONENT = 'component="window-activation"'
TERMINAL = {"confirmed", "failed", "cancelled"}
HEADER = re.compile(r'\bpid=(\d+)\b.*\bevent="([^"]+)".*\bdropped=(\d+)\b.*\bdata=(".*")$')


def read_events(paths):
    events = []
    for path in paths:
        if not path.exists():
            continue
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if 'component="app" event="start"' in line:
                match = re.search(r'\bpid=(\d+)\b', line)
                if match:
                    events.append({"process_pid":int(match.group(1)),"event":"process-start","dropped":0,"data":{}})
            if COMPONENT not in line:
                continue
            match = HEADER.search(line)
            if not match:
                raise ValueError(f"无法解析激活日志：{path}:{number}")
            pid, event, dropped, encoded = match.groups()
            data = json.loads(json.loads(encoded))
            if not isinstance(data, dict) or not isinstance(data.get("request_id"), int):
                raise ValueError(f"缺少 request_id：{path}:{number}")
            events.append({"process_pid": int(pid), "event": event,
                           "dropped": int(dropped), "data": data})
    return events


def verify(events, bundle=None, request="last", process_pid=None):
    if process_pid is None and request == "last":
        starts = [e for e in events if e["event"] == "process-start"]
        if starts:
            process_pid = starts[-1]["process_pid"]
    requested = [e for e in events if e["event"] == "requested"
                 and (bundle is None or e["data"].get("bundle_id") == bundle)]
    if process_pid is not None:
        requested = [e for e in requested if e["process_pid"] == process_pid]
    if request != "last":
        requested = [e for e in requested if str(e["data"]["request_id"]) == request]
    if not requested:
        return 2, {"result": "incomplete", "winlane_pid": process_pid, "bundle_id": bundle, "reason": "当前选定进程没有匹配的请求起点；请确认系统窗口控制权限、Info 日志和路径，并实际切换一次。"}
    start = requested[-1]
    request_id, process_pid = start["data"]["request_id"], start["process_pid"]
    chain = [e for e in events if e["process_pid"] == process_pid
             and e["data"].get("request_id") == request_id]
    summary = {"request_id": request_id, "winlane_pid": process_pid,
               "test_mode": start["data"].get("test_mode", False),
               "bundle_id": start["data"].get("bundle_id"),
               "target_pid": start["data"].get("target_pid"),
               "events": [e["event"] for e in chain]}
    terminals = [e for e in chain if e["event"] in TERMINAL]
    if not terminals or any(e["dropped"] for e in chain):
        return 2, {**summary, "result": "incomplete", "reason": "请求尚未结束、日志已轮转缺失或队列丢失记录；不能断言成功。"}
    if len(terminals) != 1 or sum(e["event"] == "requested" for e in chain) != 1:
        return 1, {**summary, "result": "invalid", "reason": "同一请求存在多个起点或终态。"}
    terminal = terminals[0]
    data = terminal["data"]
    summary.update({"elapsed_ms": data.get("elapsed_ms"), "front_pid": data.get("front_pid"),
                    "focused_id": data.get("focused_id"), "on_screen": data.get("on_screen")})
    # A native completion may arrive after cancellation. It must only be logged
    # as ignored; no app-owned retry/raise may be scheduled after a terminal.
    index = chain.index(terminal)
    if any(e["event"] in {"raise-requested", "fallback-requested", "cooperative-result", "confirmed"}
           for e in chain[index + 1:]):
        return 1, {**summary, "result": "invalid", "reason": "请求结束后仍安排了激活/置前操作。"}
    if terminal["event"] == "cancelled":
        return 3, {**summary, "result": "cancelled", "reason": data.get("reason")}
    if terminal["event"] == "failed":
        return 1, {**summary, "result": "failed", "reason": data.get("reason"),
                   "ax_error": data.get("ax_error"), "error_domain": data.get("error_domain"),
                   "error_code": data.get("error_code")}
    window = start["data"].get("window_id")
    server = start["data"].get("server_id")
    identity_matches = (window is not None and data.get("focused_id") == window) or (
        server is not None and data.get("focused_server_id") == server)
    valid = (data.get("front_pid") == start["data"].get("target_pid")
             and data.get("active") is True and data.get("running") is True
             and identity_matches and data.get("alive") is not False
             and data.get("on_screen") is not False
             and isinstance(data.get("stable_ms"), (int, float)) and data["stable_ms"] >= 50)
    observed = [e["data"] for e in chain if e["event"] == "observed"]
    # 'observed' is emitted on a change; the terminal contains the second fresh
    # sample. Require an earlier matching observation at least 50ms beforehand.
    corroborated = any(o.get("front_pid") == data.get("target_pid")
                      and o.get("active") is True and o.get("running") is True
                      and ((window is not None and o.get("focused_id") == window)
                           or (server is not None and o.get("focused_server_id") == server))
                      and o.get("on_screen") is not False and o.get("alive") is not False
                      and data.get("elapsed_ms", 0) - o.get("elapsed_ms", 0) >= 50
                      for o in observed)
    if not valid:
        return 1, {**summary, "result": "invalid", "reason": "confirmed 缺少或不符合前台、活动状态、所选窗口身份、稳定时间条件；不能当作激活成功。"}
    if not corroborated:
        return 2, {**summary, "result": "incomplete", "reason": "缺少至少早 50ms 的匹配观测；日志可能丢失，不能证明稳定确认。"}
    return 0, {**summary, "result": "confirmed", "stable_ms": data["stable_ms"],
               "visibility_verified": data.get("on_screen") is True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", action="append", type=Path, help="日志文件，可重复；默认读取配置默认位置的当前及上一个日志")
    parser.add_argument("--winlane-pid", type=int, help="显式选择 Winlane 进程；默认 last 检查当前运行进程，数字 request-id 可检查历史")
    parser.add_argument("--bundle-id", help="仅检查指定应用，例如 5ZSL2CJU2T.com.dingtalk.mac")
    parser.add_argument("--request-id", default="last", help="默认 last；数字选择该 ID 的最近一个 Winlane 进程")
    args = parser.parse_args()
    if args.winlane_pid is not None and args.winlane_pid <= 0:
        parser.error("--winlane-pid 必须为正整数")
    if args.request_id != "last" and not args.request_id.isdecimal():
        parser.error("--request-id 必须为 last 或数字")
    paths = args.log or [Path.home() / "Library/Logs/Winlane/winlane.previous.log",
                         Path.home() / "Library/Logs/Winlane/winlane.log"]
    try:
        process_pid = args.winlane_pid
        if process_pid is None and args.request_id == "last":
            running = subprocess.run(["pgrep", "-x", "winlane"], capture_output=True, text=True, timeout=5)
            if running.returncode not in (0, 1):
                raise ValueError("无法读取当前 Winlane 进程；请用 --winlane-pid 指定进程。")
            pids = [int(pid) for pid in running.stdout.split() if pid.isdecimal()]
            if len(pids) != 1:
                raise ValueError("Winlane 未运行或同时存在多个进程；请用 --winlane-pid 指定进程，历史记录请用 --request-id。")
            process_pid = pids[0]
        code, summary = verify(read_events([p.expanduser() for p in paths]), args.bundle_id, args.request_id, process_pid)
    except (OSError, ValueError, TypeError, subprocess.TimeoutExpired) as error:
        code, summary = 2, {"result": "incomplete", "reason": str(error)}
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return code


if __name__ == "__main__":
    sys.exit(main())
