#!/usr/bin/env bash
# The two audits scripts/certify runs on what it asks Lean to accept. Each fails closed.
#
#   tools/certify/audit.sh scan PATH...
#       Fails, listing the lines, when a .lean file under PATH uses a construct the kernel does
#       not check: sorry, native_decide, bv_decide, ofReduceBool/ofReduceNat, implemented_by,
#       extern, unsafe, an axiom declaration or debug.skipKernelTC.
#   tools/certify/audit.sh axioms LEAN_DIR OUT MODULE... -- THEOREM...
#       Elaborates, in the Lean package LEAN_DIR, a file that imports the modules and prints each
#       theorem's statement and axioms (the report goes to OUT). Fails when a theorem depends on an
#       axiom other than propext, Classical.choice and Quot.sound, when a theorem is missing from
#       the report, and when the file does not check.
#
# LEAN_MEMORY caps the Lean process (MB, default 3000); CERTIFY_WRAP is a command prefix for it.
set -euo pipefail

die() { echo "certify: error: $*" >&2; exit 1; }
read -r -a wrap <<<"${CERTIFY_WRAP:-}"
banned='\bsorry\b|native_decide|bv_decide|ofReduceBool|ofReduceNat|implemented_by|@\[extern|^\s*((private|protected|noncomputable)\s+)*axiom\b|\bunsafe\b|debug\.skipKernelTC'

case "${1:-}" in
    scan)
        shift
        [[ $# -gt 0 ]] || die "usage: audit.sh scan PATH..."
        if hits="$(grep -rnHE "$banned" "$@" --include='*.lean')"; then
            echo "$hits" >&2
            die "Lean files use a construct the kernel does not check (above)"
        fi
        ;;
    axioms)
        shift
        [[ $# -ge 4 ]] || die "usage: audit.sh axioms LEAN_DIR OUT MODULE... -- THEOREM..."
        lean_dir=$1 out=$2
        shift 2
        imports=()
        while [[ $# -gt 0 && $1 != -- ]]; do imports+=("$1"); shift; done
        [[ $# -gt 0 ]] || die "audit.sh axioms: no -- before the theorems"
        shift
        thms=("$@")
        [[ ${#imports[@]} -gt 0 && ${#thms[@]} -gt 0 ]] || die "audit.sh axioms: no modules or no theorems"
        file="$out.lean"
        {
            printf 'import %s\n' "${imports[@]}"
            printf '#check @%s\n' "${thms[@]}"
            printf '#print axioms %s\n' "${thms[@]}"
        } > "$file"
        status=0
        (cd "$lean_dir" && "${wrap[@]}" lake env lean -M "${LEAN_MEMORY:-3000}" "$file") > "$out" 2>&1 || status=$?
        if [[ $status -ne 0 ]]; then cat "$out" >&2; die "the axiom audit does not check"; fi
        # Lean wraps a long report over several lines: join each continuation line (indented) to
        # the line before it.
        joined="$out.joined"
        awk 'NR > 1 && /^[[:space:]]/ { sub(/^[[:space:]]+/, " "); printf "%s", $0; next } { if (NR > 1) print ""; printf "%s", $0 } END { print "" }' "$out" > "$joined"
        grep -E "depends on axioms|does not depend on any axioms" "$joined" || true
        bad="$(grep "depends on axioms" "$joined" | sed -E 's/.*depends on axioms: \[(.*)\]/\1/' |
            tr ',' '\n' | tr -d ' ' | grep -v -x -E 'propext|Classical\.choice|Quot\.sound' | sort -u || true)"
        [[ -z "$bad" ]] || die "axioms beyond propext, Classical.choice and Quot.sound: $(echo $bad)"
        [[ "$(grep -c -E "depends on axioms|does not depend on any axioms" "$joined")" -eq ${#thms[@]} ]] ||
            die "a theorem is missing from the axiom audit"
        ;;
    *) die "usage: audit.sh scan PATH... | audit.sh axioms LEAN_DIR OUT MODULE... -- THEOREM..." ;;
esac
