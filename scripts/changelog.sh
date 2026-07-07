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
root_changelog="$root/CHANGELOG.md"

if [[ "$crate" != *crates/* ]]; then
    echo "skipping $crate" >&2
    exit 0
fi

if [ -n "$NO_GIT_CLIFF" ]; then
    exit 0
fi

command=(git cliff --config "$root/cliff.toml" --unreleased "${@}")
pushd "$root" >/dev/null
if [ -z "$(git status --porcelain -- CHANGELOG.md)" ]; then
    run_unless_dry_run "${command[@]}" --prepend "$root_changelog"
else
    echo "$root_changelog has already been generated" >&2
fi
if [ -n "$crate" ] && [ "$root" != "$crate" ]; then
    crate_changelog="$crate/CHANGELOG.md"
    if [ ! -f "$crate_changelog" ]; then
        echo "missing changelog: $crate_changelog" >&2
        exit 1
    fi
    if [ "$(realpath "$crate_changelog")" = "$(realpath "$root_changelog")" ]; then
        echo "$crate_changelog uses $root_changelog" >&2
    else
        run_unless_dry_run "${command[@]}" --include-path "$crate_glob" --prepend "$crate_changelog"
    fi
fi
popd >/dev/null
