#!/bin/sh
# Yunta installer.
#
# Precursor of RFC-0004 §4.2's full installer (T12.3): that one downloads a
# precompiled release artifact and verifies its SHA-256 against
# checksums.txt published alongside a GitHub release. No release exists
# yet (that's M12's own job, T12.2) — this script builds the same target
# from source instead, but keeps the rest of the RFC's contract exactly:
# no sudo, installs to ~/.local/bin (or $YUNTA_INSTALL_DIR), warns about
# PATH, and closes by suggesting `yunta doctor`. T12.3 is expected to grow
# a "download the release if one exists, else fall back to this" path
# rather than replace it outright.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/eulke/yunta/main/install.sh | sh
# or, from a clone already on disk:
#   ./install.sh

set -eu

REPO_URL="https://github.com/eulke/yunta"
INSTALL_DIR="${YUNTA_INSTALL_DIR:-$HOME/.local/bin}"

say() {
    printf '%s\n' "$*"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "\`$1\` is required to install yunta from source"
}

# 1. Detect platform, mapped to the same target triples RFC-0004 §4.1's
#    build matrix uses — musl for Linux, so the eventual switch to a
#    downloaded prebuilt binary changes nothing about what this script
#    reports or installs.
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Linux)
        case "$arch" in
            x86_64) target="x86_64-unknown-linux-musl" ;;
            aarch64 | arm64) target="aarch64-unknown-linux-musl" ;;
            *) fail "unsupported Linux architecture: $arch" ;;
        esac
        ;;
    Darwin)
        case "$arch" in
            x86_64) target="x86_64-apple-darwin" ;;
            arm64) target="aarch64-apple-darwin" ;;
            *) fail "unsupported macOS architecture: $arch" ;;
        esac
        ;;
    *)
        fail "unsupported platform: $os/$arch — supported: Linux (x86_64, aarch64), macOS (x86_64, arm64)"
        ;;
esac
say "target: $target"

need cargo
need git

# 2. A source build needs the target's std installed — a no-op if it
#    already is. musl additionally needs a musl C toolchain on Linux
#    (`apt install musl-tools` / `brew install FiloSottile/musl-cross/musl-cross`
#    equivalents) for the static link; this script assumes one is present
#    rather than trying to install system packages itself.
if command -v rustup >/dev/null 2>&1; then
    rustup target add "$target" >/dev/null 2>&1 || true
fi

# 3. Build. If we're already inside a yunta checkout (this script run as
#    ./install.sh from a clone), build in place; otherwise shallow-clone
#    to a temp dir first, exactly what the curl-pipe usage needs.
cleanup_dir=""
cleanup() {
    [ -n "$cleanup_dir" ] && rm -rf "$cleanup_dir"
}
trap cleanup EXIT

if [ -f "Cargo.toml" ] && grep -q '^name = "yunta"' crates/cli/Cargo.toml 2>/dev/null; then
    build_dir="."
else
    cleanup_dir="$(mktemp -d)"
    say "cloning $REPO_URL..."
    git clone --depth 1 "$REPO_URL" "$cleanup_dir/yunta" >/dev/null 2>&1
    build_dir="$cleanup_dir/yunta"
fi

say "building (this compiles the whole workspace — a couple of minutes)..."
(cd "$build_dir" && cargo build --release --target "$target" -p yunta) \
    || fail "build failed — see the compiler output above"

built_binary="$build_dir/target/$target/release/yunta"
[ -f "$built_binary" ] || fail "build succeeded but $built_binary is missing"

# 4. Install — no sudo, never touches anything outside $INSTALL_DIR.
mkdir -p "$INSTALL_DIR"
install -m 755 "$built_binary" "$INSTALL_DIR/yunta"
say "installed: $INSTALL_DIR/yunta"

# 5. PATH check.
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        say ""
        say "$INSTALL_DIR is not on your PATH. Add this to your shell profile:"
        say ""
        say "    export PATH=\"$INSTALL_DIR:\$PATH\""
        say ""
        ;;
esac

# 6. Close with version + the suggested next command (RFC-0004 §4.2 step 7).
"$INSTALL_DIR/yunta" --version
say ""
say "next: run \`yunta doctor\` to see which coding agents it found."
