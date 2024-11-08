#!/usr/bin/env bash
set -eo pipefail

run_unless_dry_run() {
    if [ "$DRY_RUN" = "true" ]; then
        echo "skipping due to dry run: $*" >&2
    else
        "$@"
    fi
}

root=$WORKSPACE_ROOT
crate=$CRATE_ROOT
crate_glob="${crate#"$root/"}/**"

if [[ "$CRATE_ROOT" != *crates/* ]]; then
    exit 0
fi

command=(git cliff --workdir "$root" --config "$root/cliff.toml" --latest "${@}")
run_unless_dry_run "${command[@]}" --prepend "$root/CHANGELOG.md"
if [ -n "$crate" ] && [ "$root" != "$crate" ]; then
    run_unless_dry_run "${command[@]}" --include-path "$crate_glob" --prepend "$crate/CHANGELOG.md"
fi
