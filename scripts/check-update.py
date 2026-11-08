"""Isolated Windows update/restart check with synthetic state and optional live local transfer."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time


def wait_for(check, timeout=45):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except (OSError, ValueError, KeyError):
            pass
        time.sleep(0.15)
    raise AssertionError("Timed out waiting for the isolated test app")


def read(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("app", "updater", "output-dir"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--media", type=Path, help="Synthetic MP4 larger than 512 KiB")
    parser.add_argument("--offline", action="store_true", help="Check failure recovery with a closed proxy")
    parser.add_argument("--show-progress", action="store_true")
    args = parser.parse_args()
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)  # Never overwrite an installation or a previous test.
    app = root / "WorkFlow Media.exe"
    shutil.copyfile(args.app, app)
    data = root / "data"
    data.mkdir()
    downloads = root / "downloads"
    downloads.mkdir()
    original_file = downloads / "completed.txt"
    original_file.write_bytes(b"Synthetic completed file\n")
    settings = {
        "Language": "en", "Theme": "Light", "DownloadDirectory": str(downloads),
        "Parallel": 1, "ClipboardMode": "Off", "AutomaticUpdates": False,
        "StartWithWindows": False, "CloseToTray": True, "StartOnAdd": False,
        "QueuePaused": not bool(args.media), "CookiesBrowser": None,
    }
    preset = root / "WorkFlow Media.preset.json"
    preset.write_text(json.dumps(dict(Version=1, SetupCompleted=True, **settings)), encoding="utf-8")
    preset_bytes = preset.read_bytes()
    jobs = [
        {"Id": "manual", "State": "Paused", "Message": "@paused", "Title": "Manual pause"},
        {"Id": "previous", "State": "Paused", "Message": "@interrupted", "Title": "Earlier interruption"},
        {"Id": "done", "State": "Completed", "FilePath": str(original_file), "Title": "Completed fixture"},
    ]
    server = None
    ranges = []
    payload = None
    if args.media:
        assert not args.offline, "Use separate output directories for transfer and offline checks"
        payload = args.media.read_bytes()
        assert len(payload) > 512 * 1024

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_HEAD(self):
                self.send(False)

            def do_GET(self):
                self.send(True)

            def send(self, body):
                start = int(self.headers.get("Range", "bytes=0-").split("=")[1].split("-")[0])
                ranges.append(start)
                self.send_response(206 if start else 200)
                self.send_header("Content-Type", "video/mp4")
                self.send_header("Content-Length", str(len(payload) - start))
                self.send_header("Accept-Ranges", "bytes")
                self.send_header("ETag", '"synthetic-update-test"')
                if start:
                    self.send_header("Content-Range", f"bytes {start}-{len(payload)-1}/{len(payload)}")
                self.end_headers()
                if body:
                    try:
                        for offset in range(start, len(payload), 16384):
                            self.wfile.write(payload[offset:offset + 16384])
                            self.wfile.flush()
                            time.sleep(0.08)
                    except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                        pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        jobs.append({"Id": "active", "State": "Queued", "Source": "Other", "Format": "MP4",
                     "Directory": str(downloads), "Title": "Synthetic transfer",
                     "Url": f"http://127.0.0.1:{server.server_port}/fixture.mp4"})
    else:
        jobs.append({"Id": "queued", "State": "Queued", "Title": "Globally paused queue",
                     "Url": "https://example.com/fixture.mp4", "Directory": str(downloads)})
    queue = data / "queue.json"
    queue.write_text(json.dumps({"Version": 1, "Settings": settings, "Jobs": jobs}), encoding="utf-8")
    child = subprocess.Popen([str(app), "--isolated", "--no-uac", "--data", str(data)],
                             creationflags=subprocess.CREATE_NO_WINDOW)
    instance_file = data / "running-instance.json"
    try:
        before = wait_for(lambda: read(instance_file))
        assert before["pid"] == child.pid
        if args.media:
            wait_for(lambda: any(j["Id"] == "active" and j["State"] == "Downloading"
                                 and j["Downloaded"] > 65536 for j in read(queue)["Jobs"]), 180)
        env = os.environ.copy()
        if args.offline:
            env.update(HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1",
                       ALL_PROXY="http://127.0.0.1:1", NO_PROXY="127.0.0.1,localhost")
        command = [str(args.updater.resolve()), "--data", str(data)]
        if not args.show_progress:
            command.append("--headless")
        # File handles avoid waiting for pipe EOF from a Windows GUI descendant.
        with (root / "helper.out").open("wb") as out, (root / "helper.err").open("wb") as err:
            result = subprocess.run(command, stdout=out, stderr=err, env=env, timeout=360,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
        assert (result.returncode != 0) == args.offline, (root / "helper.err").read_text(errors="replace")
        child.wait(timeout=15)
        after = wait_for(lambda: (v if (v := read(instance_file))["pid"] != before["pid"] else None))
        assert after["nonce"] != before["nonce"]
        if args.media:
            done = wait_for(lambda: next((j for j in read(queue)["Jobs"]
                                         if j["Id"] == "active" and j["State"] == "Completed"), None), 240)
            assert hashlib.sha256(Path(done["FilePath"]).read_bytes()).digest() == hashlib.sha256(payload).digest()
            assert any(offset > 0 for offset in ranges), "Transfer did not resume from partial bytes"
        current = read(queue)
        states = {j["Id"]: j["State"] for j in current["Jobs"]}
        assert states["manual"] == states["previous"] == "Paused"
        assert states["done"] == "Completed"
        assert current["Settings"]["QueuePaused"] == settings["QueuePaused"]
        if not args.media:
            assert states["queued"] == "Queued"
        assert preset.read_bytes() == preset_bytes
        assert original_file.read_bytes() == b"Synthetic completed file\n"
        assert not (data / "external-update-recovery.json").exists()
        report = {"restarted": True, "preset_unchanged": True, "manual_pauses_preserved": True,
                  "offline_recovery": args.offline, "resumed_transfer": bool(args.media),
                  "nonzero_ranges": sum(r > 0 for r in ranges)}
        (root / "result.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report), flush=True)
    finally:
        # Use the app's own shutdown protocol; never terminate a personal process.
        if instance_file.exists():
            instance = read(instance_file)
            assert Path(instance["exe"]).resolve() == app.resolve()
            (data / "update-stop.json").write_text(json.dumps(instance), encoding="utf-8")
            wait_for(lambda: not instance_file.exists(), 120)
        if server:
            server.shutdown()


if __name__ == "__main__":
    main()
