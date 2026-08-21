#!/bin/sh
# Yunta installer (RFC-0004 §4.2).
#
# Prefers a precompiled binary from a GitHub release, verified by SHA-256
# against that release's checksums.txt before anything is extracted. If no
# release exists yet for the resolved version/platform (T12.2's pipeline
# hasn't published one, or none matches this target), falls back to
# building from source — the whole path T12.3 replaced. Either way: no
# sudo, installs to ~/.local/bin (or $YUNTA_INSTALL_DIR), warns about PATH,
# and closes by suggesting `yunta doctor`.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/eulke/yunta/main/install.sh | sh
# or, from a clone already on disk:
#   ./install.sh
#
# YUNTA_VERSION pins a release tag (e.g. "v0.3.0") instead of the latest.

set -eu

REPO_URL="https://github.com/eulke/yunta"
API_URL="https://api.github.com/repos/eulke/yunta"
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

cleanup_dir=""
cleanup() {
    [ -n "$cleanup_dir" ] && rm -rf "$cleanup_dir"
}
trap cleanup EXIT

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        fail "need \`sha256sum\` or \`shasum\` to verify the downloaded release"
    fi
}

# 2. Prefer a real release. Resolves `$YUNTA_VERSION` (a tag like
#    "v0.3.0") or the latest release via the GitHub API, downloads its
#    archive for this target plus checksums.txt, and verifies SHA-256
#    *before* extracting anything — a mismatch aborts the whole install
#    rather than falling back to a source build, since it isn't a "no
#    release yet" situation but a "the download isn't trustworthy" one.
# Returns 1 (no release available for this version/target) so the caller
# falls back to building from source; any other failure is fatal.
try_download() {
    need curl
    need tar

    tag="${YUNTA_VERSION:-}"
    if [ -z "$tag" ]; then
        tag="$(curl -fsS "$API_URL/releases/latest" 2>/dev/null | grep -m1 '"tag_name"' | cut -d'"' -f4)"
        [ -n "$tag" ] || return 1
    fi
    version="${tag#v}"

    ext="tar.gz"
    asset="yunta-${version}-${target}.${ext}"
    base_url="$REPO_URL/releases/download/$tag"

    cleanup_dir="$(mktemp -d)"
    if ! curl -fsSL -o "$cleanup_dir/$asset" "$base_url/$asset" 2>/dev/null; then
        return 1
    fi
    if ! curl -fsSL -o "$cleanup_dir/checksums.txt" "$base_url/checksums.txt" 2>/dev/null; then
        return 1
    fi

    expected="$(grep "  *$asset\$" "$cleanup_dir/checksums.txt" | cut -d' ' -f1)"
    [ -n "$expected" ] || fail "checksums.txt for $tag has no entry for $asset — refusing to install an unverifiable binary"
    actual="$(sha256 "$cleanup_dir/$asset")"
    [ "$expected" = "$actual" ] || fail "SHA-256 mismatch for $asset: expected $expected, got $actual"

    say "downloaded and verified: $asset ($tag)"
    tar xzf "$cleanup_dir/$asset" -C "$cleanup_dir"
    built_binary="$cleanup_dir/yunta-${version}-${target}/yunta"
    [ -f "$built_binary" ] || fail "$asset extracted but its yunta binary is missing"
    return 0
}

# 3. Fall back to a source build — no release published yet for this
#    version/target. Needs the target's std installed (musl additionally
#    needs a musl C toolchain on Linux — `apt install musl-tools` /
#    `brew install FiloSottile/musl-cross/musl-cross` — assumed present
#    rather than installed by this script) and shallow-clones to a temp
#    dir unless already run from inside a yunta checkout.
build_from_source() {
    need cargo
    need git

    if command -v rustup >/dev/null 2>&1; then
        rustup target add "$target" >/dev/null 2>&1 || true
    fi

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
}

if try_download; then
    :
else
    say "no matching release found — building from source instead"
    build_from_source
fi

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
