#!/usr/bin/env bash
# PGO+BOLT optimized build script for cargo-dist integration.
# All output goes to stderr except the final cargo build which uses JSON output for cargo-dist.
#
# On Linux x86_64: Does full PGO+BOLT optimization
# On other platforms: Falls back to regular cargo build (PGO/BOLT not supported)
set -euo pipefail

# Redirect all output to stderr by default
exec 3>&1 1>&2

cd "$(dirname "$0")/../.."

# Parse cargo-dist args passed via environment or command line
# Expected format: build --profile dist --message-format=json-render-diagnostics --target <triple> ...
CARGO_DIST_ARGS="${CARGO_DIST_ARGS:-}"

# Extract useful info from cargo-dist args
TARGET=""
PROFILE="dist"
FEATURES=""
MESSAGE_FORMAT=""
OTHER_ARGS=()

parse_args() {
    local args=($CARGO_DIST_ARGS)
    local i=0
    while [[ $i -lt ${#args[@]} ]]; do
        local arg="${args[$i]}"
        case "$arg" in
            build)
                # Skip the 'build' subcommand
                ;;
            --target)
                i=$((i + 1))
                TARGET="${args[$i]}"
                ;;
            --target=*)
                TARGET="${arg#--target=}"
                ;;
            --profile)
                i=$((i + 1))
                PROFILE="${args[$i]}"
                ;;
            --profile=*)
                PROFILE="${arg#--profile=}"
                ;;
            --features)
                i=$((i + 1))
                if [[ -n "$FEATURES" ]]; then
                    FEATURES="$FEATURES,${args[$i]}"
                else
                    FEATURES="${args[$i]}"
                fi
                ;;
            --features=*)
                local feat="${arg#--features=}"
                if [[ -n "$FEATURES" ]]; then
                    FEATURES="$FEATURES,$feat"
                else
                    FEATURES="$feat"
                fi
                ;;
            --message-format=*)
                MESSAGE_FORMAT="$arg"
                ;;
            *)
                OTHER_ARGS+=("$arg")
                ;;
        esac
        i=$((i + 1))
    done
}

parse_args

# Fallback to detecting target from rustc if not provided
if [[ -z "$TARGET" ]]; then
    TARGET=$(rustc -Vv | grep host | cut -d' ' -f2)
fi

# Default features if not provided
if [[ -z "$FEATURES" ]]; then
    FEATURES="cli,asm,mimalloc"
fi

LLVM_VERSION=$(rustc -Vv | grep -oP 'LLVM version: \K\d+' || echo "")

# Check if we can do PGO+BOLT (Linux x86_64 only for now)
CAN_PGO_BOLT=false
if [[ "$OSTYPE" == "linux-gnu"* ]] && [[ "$TARGET" == "x86_64-unknown-linux-gnu" ]]; then
    CAN_PGO_BOLT=true
fi

echo "=== Build ===" >&2
echo "Target: $TARGET" >&2
echo "Profile: $PROFILE" >&2
echo "Features: $FEATURES" >&2
echo "LLVM Version: $LLVM_VERSION" >&2
echo "PGO+BOLT enabled: $CAN_PGO_BOLT" >&2

CARGO_ARGS=(--profile "$PROFILE" --features "$FEATURES")

# Fallback to regular cargo build for unsupported platforms
if [[ "$CAN_PGO_BOLT" != "true" ]]; then
    echo "PGO+BOLT not supported on this platform, falling back to regular build" >&2
    # Run regular cargo build with JSON output directly to fd 3
    cargo build "${CARGO_ARGS[@]}" --message-format=json-render-diagnostics >&3
    exit 0
fi

install_bolt() {
    if command -v llvm-bolt &>/dev/null; then
        echo "BOLT already installed" >&2
        return
    fi
    echo "Installing BOLT from apt.llvm.org..." >&2
    wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc >/dev/null
    CODENAME=$(lsb_release -cs)
    echo "deb http://apt.llvm.org/$CODENAME/ llvm-toolchain-$CODENAME-$LLVM_VERSION main" | sudo tee /etc/apt/sources.list.d/llvm.list >/dev/null
    sudo apt-get update -qq
    sudo apt-get install -y -qq "bolt-$LLVM_VERSION"
    sudo ln -sf "/usr/bin/llvm-bolt-$LLVM_VERSION" /usr/local/bin/llvm-bolt
    sudo ln -sf "/usr/bin/merge-fdata-$LLVM_VERSION" /usr/local/bin/merge-fdata
}

run() {
    "$1" "${TESTDATA[@]}" --emit abi &>/dev/null || true
}

export LLVM_PROFILE_FILE=$PWD/target/pgo-profiles/solar_%m_%p.profraw

echo "Installing cargo-pgo..." >&2
cargo install cargo-pgo --quiet
rustup component add llvm-tools-preview
install_bolt
cargo pgo info

readarray -t TESTDATA < <(find testdata -maxdepth 1 -name '*.sol' ! -name 'Optimism.sol')
echo "Using ${#TESTDATA[@]} test files for profiling" >&2

# PGO: build instrumented, run, gather profiles
echo "=== PGO Phase ===" >&2
echo "Building PGO-instrumented binary..." >&2
cargo pgo build -- "${CARGO_ARGS[@]}"
echo "Running instrumented binary to gather profiles..." >&2
run "target/$TARGET/$PROFILE/solar"

# BOLT: build instrumented with PGO, run, optimize
echo "=== BOLT Phase ===" >&2
echo "Building BOLT-instrumented binary with PGO..." >&2
cargo pgo bolt build --with-pgo -- "${CARGO_ARGS[@]}"
echo "Running BOLT-instrumented binary..." >&2
run "target/$TARGET/$PROFILE/solar-bolt-instrumented"
echo "Optimizing with BOLT..." >&2
cargo pgo bolt optimize --with-pgo -- "${CARGO_ARGS[@]}"

# Copy optimized binary to expected locations
OPTIMIZED_BIN="target/$TARGET/$PROFILE/solar-bolt-optimized"
for out in "target/$TARGET/$PROFILE" "target/$PROFILE"; do
    mkdir -p "$out"
    cp "$OPTIMIZED_BIN" "$out/solar"
done

echo "=== Build Complete ===" >&2
ls -lh "target/$PROFILE/solar" >&2

# Now produce JSON output for cargo-dist on stdout (fd 3 which was original stdout)
# cargo-dist expects JSON messages from cargo build, we simulate a successful build
# by outputting the artifact path in the expected format
FINAL_BIN="target/$TARGET/$PROFILE/solar"
echo "Producing JSON output for cargo-dist..." >&2

# Output JSON message to original stdout for cargo-dist to parse
cat >&3 <<EOF
{"reason":"compiler-artifact","package_id":"solar-compiler","manifest_path":"$PWD/crates/compiler/Cargo.toml","target":{"kind":["bin"],"crate_types":["bin"],"name":"solar","src_path":"$PWD/crates/compiler/src/main.rs","edition":"2021","doc":true,"doctest":false,"test":true},"profile":{"opt_level":"3","debuginfo":0,"debug_assertions":false,"overflow_checks":false,"test":false},"features":["cli","asm","mimalloc"],"filenames":["$PWD/$FINAL_BIN"],"executable":"$PWD/$FINAL_BIN","fresh":false}
{"reason":"build-finished","success":true}
EOF
