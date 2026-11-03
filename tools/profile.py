#!/usr/bin/env python3
"""Measure Verdant's process CPU/RSS on Linux, excluding startup.

Run inside `nix develop`: python tools/profile.py --seconds 10 -- --fps 12
CPU is percent of one logical CPU; GPU/host-compositor costs are not included.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def sample(pid):
    stat = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1].split()
    ticks = int(stat[11]) + int(stat[12])
    status = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/status").read_text().splitlines())
    return ticks, int(status["VmRSS"].split()[0]) / 1024


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/verdant")
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--pause", action="store_true", help="Pause after warmup to measure idle use")
    parser.add_argument("args", nargs=argparse.REMAINDER)
    options = parser.parse_args()
    if options.seconds <= 0:
        parser.error("--seconds must be positive")
    extra = options.args[1:] if options.args[:1] == ["--"] else options.args
    process = subprocess.Popen([options.binary, *extra], stderr=subprocess.PIPE, text=True)
    try:
        time.sleep(2)
        if process.poll() is not None:
            raise RuntimeError(process.stderr.read())
        if options.pause:
            process.send_signal(signal.SIGUSR1)
            time.sleep(0.5)
        start_ticks, _ = sample(process.pid)
        start = time.monotonic()
        memory = []
        while time.monotonic() - start < options.seconds:
            time.sleep(min(0.25, options.seconds))
            end_ticks, rss = sample(process.pid)
            memory.append(rss)
        elapsed = time.monotonic() - start
        print(json.dumps({
            "arguments": extra, "paused": options.pause,
            "measured_seconds": round(elapsed, 2),
            "cpu_percent_one_core": round((end_ticks - start_ticks) / os.sysconf("SC_CLK_TCK") / elapsed * 100, 2),
            "rss_mib_mean": round(sum(memory) / len(memory), 2),
            "rss_mib_max": round(max(memory), 2),
        }, indent=2))
    finally:
        if process.poll() is None:
            process.terminate()
        try:
            _, log = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            _, log = process.communicate()
        print(log, end="")
    if process.returncode:
        raise SystemExit(process.returncode)


if __name__ == "__main__":
    main()
