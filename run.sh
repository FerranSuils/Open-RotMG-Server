#!/bin/sh
set -eu
if ! command -v python3 >/dev/null 2>&1; then
    echo "Python 3 not found. Install Python 3 and Rust/Cargo, then rerun ./run.sh." >&2
    exit 1
fi
exec python3 "$(dirname "$0")/scripts/run.py" "$@"
