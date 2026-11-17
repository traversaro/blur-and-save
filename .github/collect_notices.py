"""Collect the license notices for the Windows release alongside the executable."""

import json
from pathlib import Path
import subprocess
import sys


def main():
    root = Path(__file__).resolve().parent.parent
    result = subprocess.check_output(
        [
            "pixi", "run", "--locked", "cargo", "metadata", "--locked",
            "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc",
        ],
        cwd=root,
    )
    metadata = json.loads(result)
    selected = {node["id"] for node in metadata["resolve"]["nodes"]}
    parts = [(root / "LICENSE").read_text(encoding="utf-8")]
    parts.append("UltraFace\n\n" + (root / "assets/ULTRAFACE-LICENSE.txt").read_text(encoding="utf-8"))
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        if package["id"] not in selected or package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent
        candidates = set()
        for folder in (directory, directory / "fonts", directory / "licenses"):
            if folder.is_dir():
                candidates.update(
                    path for path in folder.iterdir()
                    if path.is_file() and (
                        path.name.upper().startswith(
                            ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "OFL", "UFL")
                        )
                        or folder.name == "fonts" and path.suffix.lower() == ".txt"
                    )
                )
        if package.get("license_file"):
            candidates.add(directory / package["license_file"])
        parts.append(f'{package["name"]} {package["version"]} ({package.get("license") or "see notice"})')
        for path in sorted(candidates):
            parts.append(path.read_text(encoding="utf-8", errors="replace"))
    notices = root / "target/release/THIRD-PARTY-NOTICES.txt"
    notices.write_text("\n\n".join(part.rstrip() for part in parts) + "\n", encoding="utf-8")
    print(notices, file=sys.stderr)


if __name__ == "__main__":
    main()
