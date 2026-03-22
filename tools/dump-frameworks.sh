#!/bin/bash
#
# dump-frameworks.sh
#
# Dumps all exported symbols from macOS frameworks to JSON.
# Safe (no runtime loading), fast, works on any macOS version.
#
# Usage:
#   ./dump-frameworks.sh > macos26-symbols.json
#   # On another machine:
#   ./dump-frameworks.sh > macos25-symbols.json
#   # Diff:
#   diff <(jq -S '.frameworks[].symbols[]' macos25-symbols.json) \
#        <(jq -S '.frameworks[].symbols[]' macos26-symbols.json) | head -100
#
# Or target specific frameworks:
#   ./dump-frameworks.sh LoggingSupport ktrace DVTInstrumentsFoundation
#

set -euo pipefail

MACOS_VERSION=$(sw_vers -productVersion)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Collect framework paths
declare -a FW_PATHS

if [ $# -gt 0 ]; then
    # Specific frameworks
    for name in "$@"; do
        for search in \
            "/System/Library/PrivateFrameworks/${name}.framework" \
            "/System/Library/Frameworks/${name}.framework" \
            "/Applications/Xcode.app/Contents/SharedFrameworks/${name}.framework"; do
            if [ -d "$search" ]; then
                FW_PATHS+=("$search")
                break
            fi
        done
    done
else
    # All frameworks
    for dir in /System/Library/PrivateFrameworks/*.framework \
               /System/Library/Frameworks/*.framework; do
        [ -d "$dir" ] && FW_PATHS+=("$dir")
    done
    # Xcode frameworks
    xcode_shared="/Applications/Xcode.app/Contents/SharedFrameworks"
    if [ -d "$xcode_shared" ]; then
        for dir in "$xcode_shared"/*.framework; do
            [ -d "$dir" ] && FW_PATHS+=("$dir")
        done
    fi
fi

echo "Scanning ${#FW_PATHS[@]} frameworks..." >&2

# Start JSON
printf '{\n  "macosVersion": "%s",\n  "timestamp": "%s",\n  "frameworks": [\n' \
    "$MACOS_VERSION" "$TIMESTAMP"

first_fw=true
count=0
total=${#FW_PATHS[@]}

for fw_path in "${FW_PATHS[@]}"; do
    count=$((count + 1))
    name=$(basename "$fw_path" .framework)

    # Progress
    if [ $((count % 100)) -eq 0 ]; then
        echo "  [$count/$total] $name" >&2
    fi

    # Get symbols via dyld_info (safe, no loading)
    symbols=$(dyld_info -exports "$fw_path" 2>/dev/null || true)
    if [ -z "$symbols" ]; then
        continue
    fi

    # Extract ObjC classes
    objc_classes=$(echo "$symbols" | grep '_OBJC_CLASS_\$_' | sed 's/.*_OBJC_CLASS_\$_//' | sort -u)
    # Extract C functions (non-ObjC, non-Swift)
    c_funcs=$(echo "$symbols" | grep -v 'OBJC_\|^\$s\|^_\$s' | awk '{print $2}' | grep '^_[a-z]' | sed 's/^_//' | sort -u)
    # Extract Swift symbols (demangled)
    swift_syms=$(echo "$symbols" | grep '^\$s\|^_\$s' | awk '{print $2}' | head -200)
    swift_demangled=""
    if [ -n "$swift_syms" ]; then
        swift_demangled=$(echo "$swift_syms" | swift demangle 2>/dev/null | sort -u || true)
    fi

    # Count
    class_count=$(echo "$objc_classes" | grep -c . || true)
    func_count=$(echo "$c_funcs" | grep -c . || true)
    swift_count=$(echo "$swift_demangled" | grep -c . || true)

    if [ "$class_count" -eq 0 ] && [ "$func_count" -eq 0 ] && [ "$swift_count" -eq 0 ]; then
        continue
    fi

    if [ "$first_fw" = true ]; then
        first_fw=false
    else
        printf ',\n'
    fi

    # Emit JSON for this framework
    printf '    {\n      "name": "%s",\n      "path": "%s",\n' "$name" "$fw_path"

    # ObjC classes
    printf '      "objcClasses": ['
    first=true
    while IFS= read -r cls; do
        [ -z "$cls" ] && continue
        if [ "$first" = true ]; then first=false; else printf ','; fi
        printf '"%s"' "$cls"
    done <<< "$objc_classes"
    printf '],\n'

    # C functions
    printf '      "cFunctions": ['
    first=true
    while IFS= read -r fn; do
        [ -z "$fn" ] && continue
        if [ "$first" = true ]; then first=false; else printf ','; fi
        # Escape any special chars
        printf '"%s"' "$fn"
    done <<< "$c_funcs"
    printf '],\n'

    # Swift (demangled, first 200)
    printf '      "swiftSymbols": ['
    first=true
    while IFS= read -r sym; do
        [ -z "$sym" ] && continue
        if [ "$first" = true ]; then first=false; else printf ','; fi
        # Escape quotes in demangled names
        escaped=$(echo "$sym" | sed 's/"/\\"/g')
        printf '"%s"' "$escaped"
    done <<< "$swift_demangled"
    printf '],\n'

    printf '      "counts": {"objcClasses": %d, "cFunctions": %d, "swiftSymbols": %d}\n' \
        "$class_count" "$func_count" "$swift_count"
    printf '    }'
done

printf '\n  ]\n}\n'

echo "Done. Scanned $count frameworks." >&2
