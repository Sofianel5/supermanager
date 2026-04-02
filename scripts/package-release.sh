#!/usr/bin/env bash

set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <target-triple> <output-dir>" >&2
  exit 1
fi

target="$1"
output_dir="$2"
binary_path="target/${target}/release/supermanager"
archive_name="supermanager-${target}.tar.gz"
archive_path="${output_dir}/${archive_name}"

if [ ! -f "$binary_path" ]; then
  echo "missing binary: $binary_path" >&2
  exit 1
fi

mkdir -p "$output_dir"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cp "$binary_path" "$tmp_dir/supermanager"
chmod +x "$tmp_dir/supermanager"

tar -C "$tmp_dir" -czf "$archive_path" supermanager
