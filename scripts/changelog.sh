#!/usr/bin/env bash
set -eo pipefail

run_unless_dry_run() {
    if [ "$DRY_RUN" = "true" ]; then
        echo "skipping due to dry run: $*" >&2
    else
        echo "running: $*" >&2
        "$@"
    fi
}

root=$WORKSPACE_ROOT
crate=$CRATE_ROOT
crate_glob="${crate#"$root/"}/**"

if [[ "$crate" != *crates/* ]]; then
    echo "skipping $crate" >&2
    exit 0
fi

if [ -n "$NO_GIT_CLIFF" ]; then
    exit 0
fi

command=(git cliff --workdir "$root" --config "$root/cliff.toml" --unreleased "${@}")
if [ -z "$(git -C "$root" status --porcelain -- CHANGELOG.md)" ]; then
    pushd "$root" >/dev/null
    run_unless_dry_run "${command[@]}" --prepend "$root/CHANGELOG.md"
    popd >/dev/null
else
    echo "$root/CHANGELOG.md has already been generated" >&2
fi
if [ -n "$crate" ] && [ "$root" != "$crate" ]; then
    crate_changelog="$crate/CHANGELOG.md"
    if [ ! -f "$crate_changelog" ]; then
        echo "missing changelog: $crate_changelog" >&2
        exit 1
    fi
    run_unless_dry_run "${command[@]}" --include-path "$crate_glob" --prepend "$crate_changelog"
fi
