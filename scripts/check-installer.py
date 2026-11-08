"""Check actual Windows release packages; optional isolated signed-download installation."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess


def run(exe, *args):
    result = subprocess.run(
        [str(exe), *map(str, args)], capture_output=True, timeout=240,
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("app", "builder", "stub", "output-dir"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--expected-app", type=Path)
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    stub = args.stub.read_bytes()
    assert stub[:2] == b"MZ"
    assert len(stub) <= 5 * 1024 * 1024 - 65536 - 56
    # The graphical builder must not accidentally include the large media bundle either.
    assert args.builder.stat().st_size < 40 * 1024 * 1024
    profile = {
        "Version": 1, "DownloadFolder": "Media",
        "Location": {"Folder": "Documents", "Subfolder": "WorkFlow Media", "Shortcut": "Desktop"},
        "Choices": {
            "Language": "en", "Parallel": 8, "Format": "MP4", "Quality": 0,
            "StartWithWindows": False, "CloseToTray": True, "StartOnAdd": True,
            "ClipboardMode": "Selected", "ClipboardSources": ["YouTube", "Facebook", "FastShare"],
            "ServicePreferences": {}, "AutomaticUpdates": True,
            "CookiesBrowser": None, "CookiesMode": "All", "CookiesBrowsers": ["Chrome", "Brave"],
            "CookiesAutoRefresh": False,
        },
    }
    profile_file = output / "synthetic-profile.json"
    profile_file.write_text(json.dumps(profile), encoding="utf-8")
    packages = []
    for name, producer in (("builder", args.builder), ("app", args.app)):
        setup = output / (name + "-setup.exe")
        result = run(producer.resolve(), "--make-installer", "--profile", profile_file, "--output", setup)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        data = setup.read_bytes()
        assert len(data) <= 5 * 1024 * 1024, "Recipient installer exceeded 5 MiB"
        footer = data[-56:]
        assert footer[40:] == b"WFMEDIA_BUNDLE_1"
        size = struct.unpack("<Q", footer[:8])[0]
        assert 0 < size <= 65536
        payload = data[-56-size:-56]
        assert data[:-56-size] == stub, "Installer contains something other than the small stub"
        assert hashlib.sha256(payload).digest() == footer[8:40]
        assert json.loads(payload) == {"Mode": "Installer", "Profile": profile}
        result = run(setup, "--version")
        assert result.returncode == 0 and b"WorkFlow Media Setup" in result.stdout
        packages.append(data)
        print(f"{name}: recipient installer {len(data):,} bytes; stub and profile verified.", flush=True)
    assert packages[0] == packages[1], "Both generation paths must produce the same recipient installer"
    corrupted = bytearray(packages[0])
    corrupted[len(stub)] ^= 1
    bad_file = output / "damaged-setup.exe"
    bad_file.write_bytes(corrupted)
    bad_target = output / "must-not-install"
    assert not bad_target.exists()
    result = run(bad_file, "--test-install-to", bad_target)
    assert result.returncode != 0 and b"damaged" in result.stderr
    assert not bad_target.exists(), "Damaged profile must be rejected before installation"
    print("Damaged preset rejected before any installation.", flush=True)
    if args.install:
        recipient = output / "recipient"
        result = run(output / "builder-setup.exe", "--test-install-to", recipient)
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        app = recipient / "WorkFlow Media.exe"
        preset = json.loads((recipient / "WorkFlow Media.preset.json").read_text(encoding="utf-8"))
        assert preset["SetupCompleted"] is True
        for key in ("Language", "Parallel", "Format", "Quality", "StartWithWindows", "CloseToTray",
                    "StartOnAdd", "ClipboardMode", "ClipboardSources", "AutomaticUpdates", "CookiesMode",
                    "CookiesBrowsers", "CookiesAutoRefresh"):
            assert preset[key] == profile["Choices"][key], key
        assert preset["CookiesFile"] == "" and preset["CookieProfiles"] == []
        assert preset["CookiesBrowser"] is None
        if args.expected_app:
            assert hashlib.sha256(app.read_bytes()).digest() == hashlib.sha256(args.expected_app.read_bytes()).digest()
        print("Small installer downloaded the signed release and applied the prepared settings.", flush=True)


if __name__ == "__main__":
    main()
