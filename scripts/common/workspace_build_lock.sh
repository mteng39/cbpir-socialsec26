#!/usr/bin/env bash

# Shared side of the workspace cleaner/build cooperative-lock contract.
readonly CBPIR_COMMON_SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly CBPIR_REPOSITORY_ROOT="$(cd -- "$CBPIR_COMMON_SCRIPT_DIR/../.." && pwd)"
readonly CBPIR_WORKSPACE_BUILD_LOCK_PATH="${CBPIR_WORKSPACE_BUILD_LOCK:-$CBPIR_REPOSITORY_ROOT/target/locks/workspace-build.lock}"

acquire_workspace_build_lock_shared() {
    local command lock_dir old_umask mode owner
    lock_dir=${CBPIR_WORKSPACE_BUILD_LOCK_PATH%/*}

    for command in flock id mkdir realpath stat; do
        command -v "$command" >/dev/null 2>&1 || {
            printf 'required command not found: %s\n' "$command" >&2
            return 1
        }
    done
    if [[ ! -e $lock_dir ]]; then
        mkdir -p -m 700 -- "$lock_dir" || return 1
    fi
    [[ -d $lock_dir && ! -L $lock_dir ]] || {
        printf 'unsafe or missing workspace build-lock directory: %s\n' "$lock_dir" >&2
        return 1
    }
    [[ $(realpath -e -- "$lock_dir") == "$lock_dir" ]] || {
        printf 'workspace build-lock directory is redirected: %s\n' "$lock_dir" >&2
        return 1
    }
    owner=$(stat -c '%u' -- "$lock_dir") || return 1
    mode=$(stat -c '%a' -- "$lock_dir") || return 1
    [[ $owner == "$(id -u)" && $mode == 700 ]] || {
        printf 'workspace build-lock directory must be owned by this user with mode 0700\n' >&2
        return 1
    }
    [[ ! -L $CBPIR_WORKSPACE_BUILD_LOCK_PATH &&
       ( ! -e $CBPIR_WORKSPACE_BUILD_LOCK_PATH || -f $CBPIR_WORKSPACE_BUILD_LOCK_PATH ) ]] || {
        printf 'unsafe workspace build lock: %s\n' "$CBPIR_WORKSPACE_BUILD_LOCK_PATH" >&2
        return 1
    }

    old_umask=$(umask)
    umask 077
    if ! exec {CBPIR_WORKSPACE_BUILD_LOCK_FD}>>"$CBPIR_WORKSPACE_BUILD_LOCK_PATH"; then
        umask "$old_umask"
        return 1
    fi
    umask "$old_umask"

    owner=$(stat -Lc '%u' -- "/proc/$$/fd/$CBPIR_WORKSPACE_BUILD_LOCK_FD") || return 1
    mode=$(stat -Lc '%a' -- "/proc/$$/fd/$CBPIR_WORKSPACE_BUILD_LOCK_FD") || return 1
    [[ $owner == "$(id -u)" && $mode == 600 ]] || {
        printf 'workspace build lock must be owned by this user with mode 0600\n' >&2
        return 1
    }
    flock -s "$CBPIR_WORKSPACE_BUILD_LOCK_FD"
}
