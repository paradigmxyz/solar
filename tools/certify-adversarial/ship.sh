#!/usr/bin/env bash
# ship.sh NAME: copies a finished case off the box at once.
#
# It packs the case directory without its work tree (status, command, certify.log, receipt and its
# trace, the source and its sha256, rec/ with record.json, producer-check.json) with a MANIFEST of
# every file's sha256 into OUT/outbox/NAME.tgz, and, when SHIP_REMOTE is set, commits the unpacked
# copy to SHIP_BRANCH under runs/SOLAR_SHA/NAME and pushes it (one case per commit, retried after a
# rebase). OUT/outbox/NAME.shipped records the pushed commit; a failed push leaves NAME.tgz for a
# copy from outside and writes NAME.ship-failed.
set -u
name=$1
R="$OUT/$name"; ob="$OUT/outbox"; mkdir -p "$ob"
cp "$PRODUCER" "$R/producer.json"
(cd "$R" && find . -path ./work -prune -o -type f ! -name MANIFEST.sha256 -print | sort | xargs -r -d '\n' sha256sum > MANIFEST.sha256)
tar -C "$OUT" --exclude="$name/work" -czf "$ob/$name.tgz.part" "$name" && mv "$ob/$name.tgz.part" "$ob/$name.tgz"
sha256sum "$ob/$name.tgz" > "$ob/$name.tgz.sha256"
[[ -n "${SHIP_REMOTE:-}" ]] || exit 0
exec 9> "$OUT/.ship.lock"; flock 9
S="$OUT/.ship"
if [[ ! -d "$S/.git" ]]; then
    git clone -q --depth 1 --branch "$SHIP_BRANCH" "$SHIP_REMOTE" "$S" 2>/dev/null ||
        { git init -q "$S" && git -C "$S" checkout -q -b "$SHIP_BRANCH" && git -C "$S" remote add origin "$SHIP_REMOTE"; }
fi
dst="$S/runs/$SOLAR_SHA/$name"; rm -rf "$dst"; mkdir -p "$dst"
tar -C "$dst" --strip-components=1 -xzf "$ob/$name.tgz"
git -C "$S" add -A "runs/$SOLAR_SHA/$name"
git -C "$S" commit -q -m "test(certify): adversarial run $name" -m "solar $SOLAR_SHA, solidity-lean $LEAN_SHA: $(tail -1 "$R/status")" || true
for i in 1 2 3 4 5; do
    if git -C "$S" push -q origin "HEAD:$SHIP_BRANCH" 2>> "$ob/$name.ship.log"; then
        c=$(git -C "$S" rev-parse HEAD)
        if git ls-remote "$SHIP_REMOTE" "refs/heads/$SHIP_BRANCH" | grep -q "^$c"; then
            echo "$c" > "$ob/$name.shipped"; exit 0
        fi
    fi
    git -C "$S" pull -q --rebase origin "$SHIP_BRANCH" 2>> "$ob/$name.ship.log" || true
    sleep $((i * 5))
done
echo "push failed; $ob/$name.tgz is the copy" > "$ob/$name.ship-failed"
exit 1
