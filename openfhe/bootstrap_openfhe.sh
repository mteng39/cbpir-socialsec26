#!/usr/bin/env bash
set -euo pipefail

readonly OPENFHE_RS_COMMIT="3e6a46b1377e89e09414cbca7c0c0140c5338397"
readonly OPENFHE_TAG="v1.5.1"
readonly OPENFHE_CORE_COMMIT="1306d14f8c26bb6150d3e6ad54f28dfe1007689e"

readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
source "$SCRIPT_DIR/../scripts/common/workspace_build_lock.sh"
acquire_workspace_build_lock_shared
readonly DEPS_DIR="$SCRIPT_DIR/.openfhe"
readonly CORE_DIR="$DEPS_DIR/openfhe-1.5.1"
readonly BUILD_DIR="$CORE_DIR/build-capability"
readonly INSTALL_DIR="$CORE_DIR/install"
readonly BUILD_JOBS="${OPENFHE_BUILD_JOBS:-1}"
readonly WITH_OPENMP="${OPENFHE_WITH_OPENMP:-ON}"

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'required command not found: %s\n' "$1" >&2
        exit 1
    fi
}

for command in git cmake c++; do
    require_command "$command"
done

case "$BUILD_JOBS" in
    ''|*[!0-9]*|0)
        printf 'OPENFHE_BUILD_JOBS must be a positive integer\n' >&2
        exit 1
        ;;
esac

mkdir -p "$DEPS_DIR"
if [[ ! -d "$CORE_DIR/.git" ]]; then
    if [[ -e "$CORE_DIR" ]]; then
        printf '%s exists but is not a Git checkout; refusing to overwrite it\n' "$CORE_DIR" >&2
        exit 1
    fi
    git clone --branch "$OPENFHE_TAG" --filter=blob:none \
        https://github.com/openfheorg/openfhe-development.git "$CORE_DIR"
fi

git -C "$CORE_DIR" fetch --tags origin "$OPENFHE_TAG"
git -C "$CORE_DIR" checkout --detach "$OPENFHE_CORE_COMMIT"
test "$(git -C "$CORE_DIR" rev-parse HEAD)" = "$OPENFHE_CORE_COMMIT"

cmake -S "$CORE_DIR" -B "$BUILD_DIR" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$INSTALL_DIR" \
    -DBUILD_SHARED=ON \
    -DBUILD_STATIC=OFF \
    -DBUILD_UNITTESTS=OFF \
    -DBUILD_EXAMPLES=OFF \
    -DBUILD_BENCHMARKS=OFF \
    -DBUILD_EXTRAS=OFF \
    -DWITH_OPENMP="$WITH_OPENMP"
cmake --build "$BUILD_DIR" --parallel "$BUILD_JOBS"
cmake --install "$BUILD_DIR"

printf 'OpenFHE-rs Cargo pin: %s\n' "$OPENFHE_RS_COMMIT"
printf 'OpenFHE tag:          %s\n' "$OPENFHE_TAG"
printf 'OpenFHE commit:       %s\n' "$(git -C "$CORE_DIR" rev-parse HEAD)"
printf 'OpenFHE install:      %s\n' "$INSTALL_DIR"
printf 'OpenFHE OpenMP:       %s\n' "$WITH_OPENMP"
printf '\nSet OPENFHE_INSTALL_DIR=%s\n' "$INSTALL_DIR"
printf 'Set LD_LIBRARY_PATH=%s/lib\n' "$INSTALL_DIR"
