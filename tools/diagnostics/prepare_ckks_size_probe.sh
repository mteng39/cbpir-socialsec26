#!/usr/bin/env bash
# Preserve P0-C's relative fixture path and compile-time serialization location.
set -euo pipefail
p0d_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(cd -- "$p0d_dir/../../.." && pwd)"
snapshot="$p0d_dir/target/probe_workspace"
source_dir="$repo_dir/code_branch/prototypes/P0-C_ckks_dft_search"
destination="$snapshot/code_branch/prototypes/P0-C_ckks_dft_search"
if test -e "$snapshot"; then
    echo 'Snapshot already exists; verify existing contents instead of overwriting.' >&2
    exit 1
fi
mkdir -p "$destination" "$snapshot/docs/project/experiments/search/workloads"
cp -a "$source_dir/src" "$destination/src"
cp "$source_dir/Cargo.toml" "$source_dir/Cargo.lock" "$source_dir/build.rs" "$destination/"
cp "$repo_dir/docs/project/experiments/search/workloads/p0_search_workloads_v1.json" "$snapshot/docs/project/experiments/search/workloads/"
while IFS= read -r -d '' original; do
    relative="${original#"$source_dir/"}"
    cmp -- "$original" "$destination/$relative"
    sha256sum "$original" "$destination/$relative"
done < <(find "$source_dir/src" -type f -name '*.rs' -print0)
for name in Cargo.toml Cargo.lock build.rs; do
    cmp -- "$source_dir/$name" "$destination/$name"
    sha256sum "$source_dir/$name" "$destination/$name"
done
cmp "$repo_dir/docs/project/experiments/search/workloads/p0_search_workloads_v1.json" "$snapshot/docs/project/experiments/search/workloads/p0_search_workloads_v1.json"
sha256sum "$snapshot/docs/project/experiments/search/workloads/p0_search_workloads_v1.json"
