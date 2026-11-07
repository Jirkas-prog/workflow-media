"""Collect attribution files from Cargo's Windows and Linux metadata snapshots.

Run cargo metadata --locked --format-version 1 --filter-platform TARGET for both
targets and pass the two JSON filenames as arguments. All paths stay local;
only package names, public source URLs and upstream notices enter the result.
"""
import json
import sys
from pathlib import Path

packages = {}
for filename in sys.argv[1:]:
    metadata = json.loads(Path(filename).read_text(encoding="utf-8-sig"))
    active = {node["id"] for node in metadata["resolve"]["nodes"]}
    for package in metadata["packages"]:
        if package["id"] in active and package["name"] != "workflow-media":
            packages[(package["name"], package["version"])] = package

parts = ["WorkFlow Media - Rust dependency license texts and attributions\n"]
for (name, version), package in sorted(packages.items()):
    manifest = package["manifest_path"]
    if sys.platform == "win32" and manifest.startswith("/mnt/"):
        manifest = manifest[5].upper() + ":/" + manifest[7:]
    root = Path(manifest).parent
    candidates = set()
    if package.get("license_file"):
        candidates.add(root / package["license_file"])
    for pattern in ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "license*", "licence*", "copying*", "notice*"):
        candidates.update(root.glob(pattern))
    files = sorted(p for p in candidates if p.is_file())
    parts.append(f"\n{'=' * 60}\n{name} {version} | {package.get('license') or 'See upstream license'}\nSource: https://crates.io/crates/{name}/{version}\n")
    for path in files:
        parts.append(f"--- {path.name} ---\n{path.read_text(encoding='utf-8', errors='replace')}\n")
    if not files:
        parts.append("License identifier above; source and any additional notices are in the upstream crate.\n")

parts.append(Path("resources/media-LICENSES.txt").read_text(encoding="utf-8"))
Path("THIRD-PARTY-LICENSES.txt").write_text("".join(parts), encoding="utf-8")
print(f"Collected notices for {len(packages)} packages")
