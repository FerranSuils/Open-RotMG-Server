#!/bin/sh
set -eu
client="$(dirname "$0")/../client/run.sh"
if [ ! -f "$client" ]; then
    echo "Missing sibling client. Clone https://github.com/OpenRotMG/Open-RotMG-Client beside this repository as client." >&2
    exit 1
fi
exec sh "$client" "$@"
