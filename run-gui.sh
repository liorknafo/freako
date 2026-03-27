#!/usr/bin/env bash
set -euo pipefail

crate="freako-gui"
bin_name="freako-gui"
profile="debug"

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_bin="$root_dir/target/$profile/$bin_name"
launch_root="$root_dir/target/dev-run/$bin_name"
timestamp="$(date +%Y%m%d-%H%M%S)"
launch_dir="$launch_root/$timestamp"
launch_bin="$launch_dir/$bin_name"

cargo build -p "$crate"

if [[ ! -f "$source_bin" ]]; then
  echo "Built binary not found: $source_bin" >&2
  exit 1
fi

mkdir -p "$launch_dir"
cp "$source_bin" "$launch_bin"
chmod +x "$launch_bin"

"$launch_bin" >/dev/null 2>&1 &
pid=$!

echo "Built:  $source_bin"
echo "Copied: $launch_bin"
echo "PID:    $pid"
