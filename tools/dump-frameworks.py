#!/usr/bin/env python3
"""
Dump all exported symbols from macOS SDK frameworks to JSON.

Uses TBD stub files from the Xcode SDK (no runtime loading needed).
For Xcode SharedFrameworks, uses dyld_info on the real binaries.

Usage:
    python3 dump-frameworks.py > macos26-apis.json
    python3 dump-frameworks.py LoggingSupport ktrace kperf DVTInstrumentsFoundation
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path


def parse_tbd(path: str) -> dict:
    """Parse a .tbd file and extract symbols and ObjC classes."""
    text = Path(path).read_text()

    symbols = []
    objc_classes = []

    # Extract symbols arrays (YAML-ish format)
    for m in re.finditer(r"symbols:\s*\[([^\]]+)\]", text):
        syms = [s.strip().strip("_") for s in m.group(1).split(",")]
        symbols.extend(s for s in syms if s)

    for m in re.finditer(r"objc-classes:\s*\[([^\]]+)\]", text):
        classes = [c.strip() for c in m.group(1).split(",")]
        objc_classes.extend(c for c in classes if c)

    return {
        "symbols": sorted(set(symbols)),
        "objcClasses": sorted(set(objc_classes)),
    }


def dump_binary(path: str) -> dict:
    """Dump symbols from a real Mach-O binary via dyld_info."""
    try:
        result = subprocess.run(
            ["dyld_info", "-exports", path],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode != 0:
            return {"symbols": [], "objcClasses": []}
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return {"symbols": [], "objcClasses": []}

    symbols = []
    objc_classes = []

    for line in result.stdout.splitlines():
        parts = line.strip().split()
        if len(parts) < 2:
            continue
        sym = parts[-1]
        if "OBJC_CLASS_$_" in sym:
            cls = sym.split("OBJC_CLASS_$_")[-1]
            objc_classes.append(cls)
        elif sym.startswith("_") and not sym.startswith("_$s"):
            symbols.append(sym.lstrip("_"))

    return {
        "symbols": sorted(set(symbols)),
        "objcClasses": sorted(set(objc_classes)),
    }


def find_frameworks(names=None):
    """Find framework paths in SDK and Xcode."""
    frameworks = {}

    # SDK private frameworks (TBD stubs)
    sdk_base = None
    for candidate in [
        "/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk",
        "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk",
    ]:
        if os.path.isdir(candidate):
            sdk_base = candidate
            break

    if sdk_base:
        for fwdir in [
            f"{sdk_base}/System/Library/PrivateFrameworks",
            f"{sdk_base}/System/Library/Frameworks",
        ]:
            if not os.path.isdir(fwdir):
                continue
            for entry in sorted(os.listdir(fwdir)):
                if not entry.endswith(".framework"):
                    continue
                name = entry.replace(".framework", "")
                if names and name not in names:
                    continue
                tbd = f"{fwdir}/{entry}/{name}.tbd"
                vtbd = f"{fwdir}/{entry}/Versions/A/{name}.tbd"
                if os.path.isfile(tbd):
                    frameworks[name] = ("tbd", tbd)
                elif os.path.isfile(vtbd):
                    frameworks[name] = ("tbd", vtbd)

    # Xcode SharedFrameworks (real binaries)
    xcode_shared = "/Applications/Xcode.app/Contents/SharedFrameworks"
    if os.path.isdir(xcode_shared):
        for entry in sorted(os.listdir(xcode_shared)):
            if not entry.endswith(".framework"):
                continue
            name = entry.replace(".framework", "")
            if names and name not in names:
                continue
            binary = f"{xcode_shared}/{entry}/{name}"
            vbinary = f"{xcode_shared}/{entry}/Versions/A/{name}"
            if os.path.isfile(binary):
                frameworks[name] = ("binary", binary)
            elif os.path.isfile(vbinary):
                frameworks[name] = ("binary", vbinary)

    return frameworks


def main():
    names = set(sys.argv[1:]) if len(sys.argv) > 1 else None
    frameworks = find_frameworks(names)

    print(f"Scanning {len(frameworks)} frameworks...", file=sys.stderr)

    macos_version = subprocess.run(
        ["sw_vers", "-productVersion"],
        capture_output=True, text=True,
    ).stdout.strip()

    results = []
    for i, (name, (kind, path)) in enumerate(sorted(frameworks.items())):
        if i % 100 == 0 and i > 0:
            print(f"  [{i}/{len(frameworks)}]", file=sys.stderr)

        if kind == "tbd":
            data = parse_tbd(path)
        else:
            data = dump_binary(path)

        total = len(data["symbols"]) + len(data["objcClasses"])
        if total == 0:
            continue

        results.append({
            "name": name,
            "path": path,
            "source": kind,
            "objcClasses": data["objcClasses"],
            "symbols": data["symbols"],
            "counts": {
                "objcClasses": len(data["objcClasses"]),
                "symbols": len(data["symbols"]),
            },
        })

    output = {
        "macosVersion": macos_version,
        "sdkPath": frameworks and next(iter(frameworks.values()))[1].split("/System/")[0] or "",
        "frameworkCount": len(results),
        "totalObjcClasses": sum(f["counts"]["objcClasses"] for f in results),
        "totalSymbols": sum(f["counts"]["symbols"] for f in results),
        "frameworks": results,
    }

    print(f"Done. {len(results)} frameworks with symbols.", file=sys.stderr)
    json.dump(output, sys.stdout, indent=2, sort_keys=True)
    print()


if __name__ == "__main__":
    main()
