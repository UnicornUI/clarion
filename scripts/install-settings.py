#!/usr/bin/env python3
"""Publish the GPUI system settings app locally and remove the retired egui app."""
import argparse
from datetime import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile


def sha256(path):
    return hashlib.file_digest(path.open("rb"), "sha256").hexdigest()


def atomic_text(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=".clarion-settings-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as stream:
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True,
                        help="clarion-settings build to install")
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".local")
    arguments = parser.parse_args()
    binary = arguments.binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error("--binary must be an executable file")
    prefix = arguments.prefix.resolve()
    root = prefix / "lib/clarion-settings"
    legacy_root = prefix / "lib/clarion-system-settings"
    root.mkdir(parents=True, exist_ok=True)
    tag = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    launcher = prefix / "bin/clarion-settings"
    desktop = prefix / "share/applications/clarion-settings.desktop"
    icon = prefix / "share/icons/hicolor/256x256/apps/clarion-settings.png"
    icon_source = Path(__file__).resolve().parent.parent / "clarion-settings/assets/clarion-settings.png"
    legacy_paths = [
        prefix / "bin/clarion-system-settings",
        prefix / "bin/realand-settings",
        prefix / "share/applications/clarion-system-settings.desktop",
    ]
    with (root / ".install.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        backup = root / "backups" / tag
        backup.mkdir(parents=True)
        previous = {}
        for name, path in {
            "launcher": launcher,
            "desktop": desktop,
            "icon": icon,
        }.items():
            if path.is_symlink():
                target = os.readlink(path)
                (backup / name).symlink_to(target)
                previous[name] = {"path": str(path), "target": target}
            elif path.exists():
                shutil.copy2(path, backup / name)
                previous[name] = {"path": str(path), "copy": str(backup / name)}
            else:
                previous[name] = {"path": str(path), "absent": True}
        removed_legacy = []
        for path in legacy_paths:
            if path.exists() or path.is_symlink():
                path.unlink()
                removed_legacy.append(str(path))
        release = root / "releases" / tag
        release.mkdir(parents=True)
        installed = release / "clarion-settings"
        shutil.copy2(binary, installed)
        installed.chmod(0o755)
        digest = sha256(binary)
        if sha256(installed) != digest:
            raise RuntimeError("Copied executable differs from the build")
        temporary = launcher.with_name(f".clarion-settings-{tag}")
        temporary.symlink_to(installed)
        launcher.parent.mkdir(parents=True, exist_ok=True)
        os.replace(temporary, launcher)
        icon.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(icon_source, icon)
        escaped = str(launcher).replace("\\", "\\\\").replace('"', '\\"').replace("`", "\\`").replace("$", "\\$").replace("%", "%%%%")
        atomic_text(desktop, f'''[Desktop Entry]
Type=Application
Name=系统设置
Name[en]=Clarion Settings
Comment=Configure the Clarion desktop
Exec="{escaped}"
Icon=clarion-settings
Terminal=false
Categories=Settings;DesktopSettings;
StartupNotify=true
StartupWMClass=clarion-settings
''')
        for directory in (root / "releases", root / "backups"):
            if directory.is_dir():
                entries = sorted(entry for entry in directory.iterdir() if entry.is_dir())
                for stale in entries[:-2]:
                    shutil.rmtree(stale, ignore_errors=True)
        removed_legacy_root = legacy_root.is_dir() and not legacy_root.is_symlink()
        if removed_legacy_root:
            shutil.rmtree(legacy_root, ignore_errors=True)
        receipt = {
            "release": str(release),
            "removed_legacy_root": str(legacy_root) if removed_legacy_root else None,
            "binary": str(installed),
            "sha256": digest,
            "previous": previous,
            "removed_legacy": removed_legacy,
            "backup": str(backup),
        }
        atomic_text(release / "installation.json", json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps(receipt, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
