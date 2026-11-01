#!/usr/bin/env bash
# Builds the web frontend's bundle: index.html, blaulicht_web.js and
# blaulicht_web_bg.wasm, plus .gz copies the engine serves to browsers that
# take gzip.
#
#   crates/web/build.sh [--dev] [--lint] [OUT_DIR]
#
# OUT_DIR defaults to $CARGO_TARGET_DIR/web (target/web), which the engine
# finds on its own when it runs from target/debug or target/release; otherwise
# copy the folder next to the binary as web/, or set web_ui_dir in config.toml.
#   --dev   unoptimized (fast to build, large and slow in the browser)
#   --lint  first check that no code of the engine or the egui crate uses
#           std::time's clocks, which panic in a browser (clippy, wasm32)
#
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli of exactly the
# wasm-bindgen version in Cargo.lock (the script prints the install command).
set -euo pipefail
cd "$(dirname "$0")/../.."

profile=web lint=0 out=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dev) profile=dev ;;
        --lint) lint=1 ;;
        -h | --help) sed -n '2,17p' "$0"; exit 0 ;;
        -*) echo "build.sh: unknown option $1" >&2; exit 2 ;;
        *) out="$1" ;;
    esac
    shift
done
target_dir="${CARGO_TARGET_DIR:-target}"
out="${out:-$target_dir/web}"
export PATH="$PATH:${CARGO_HOME:-$HOME/.cargo}/bin"

want=$(awk '$0 == "name = \"wasm-bindgen\"" { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)
if [[ $have != "$want" ]]; then
    echo "error: wasm-bindgen-cli $want is needed (found: ${have:-none}). Install it with" >&2
    echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi

if [[ $lint -eq 1 ]]; then
    echo "==> lint: std::time clocks in wasm32 code"
    found=$(cargo clippy -p blaulicht-web --target wasm32-unknown-unknown 2>&1 \
        | grep -A4 -E 'disallowed (method|type)' || true)
    if [[ -n $found ]]; then
        echo "$found" >&2
        echo "error: use web_time instead of std::time's Instant / SystemTime (see crates/engine/clippy.toml)" >&2
        exit 1
    fi
fi

echo "==> blaulicht-web (profile $profile)"
cargo build -p blaulicht-web --target wasm32-unknown-unknown --profile "$profile"
[[ $profile == dev ]] && artifact_dir=debug || artifact_dir=$profile
wasm="$target_dir/wasm32-unknown-unknown/$artifact_dir/blaulicht_web.wasm"

rm -rf "$out"
mkdir -p "$out"
wasm-bindgen --target web --no-typescript --out-dir "$out" "$wasm"
if [[ $profile != dev ]] && command -v wasm-opt >/dev/null; then
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
        -o "$out/blaulicht_web_bg.wasm" "$out/blaulicht_web_bg.wasm"
fi
cp crates/web/index.html "$out/"
gzip -9 -k -f "$out/blaulicht_web_bg.wasm" "$out/blaulicht_web.js" "$out/index.html"

echo "==> $out"
for file in "$out"/*; do
    printf '  %-28s %10s bytes\n' "$(basename "$file")" "$(wc -c <"$file" | tr -d ' ')"
done
