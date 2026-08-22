#!/bin/sh
# Polls crates.io's sparse index for a crate/version to actually appear.
# `cargo publish` returns as soon as the upload is accepted, not once the
# index (and therefore dependency resolution for the next crate in a
# dependency-ordered publish sequence) reflects it — publishing in
# dependency order needs this wait, not a fixed sleep.
#
# Usage: wait-for-crate-index.sh <crate-name> <version>

set -eu

name="$1"
version="$2"

# crates.io sparse index path scheme (https://doc.rust-lang.org/cargo/reference/registry-index.html):
#   1 char  -> 1/<name>
#   2 chars -> 2/<name>
#   3 chars -> 3/<first-char>/<name>
#   4+      -> <first-2>/<next-2>/<name>
len=${#name}
if [ "$len" -eq 1 ]; then
    path="1/$name"
elif [ "$len" -eq 2 ]; then
    path="2/$name"
elif [ "$len" -eq 3 ]; then
    path="3/$(printf '%s' "$name" | cut -c1)/$name"
else
    path="$(printf '%s' "$name" | cut -c1-2)/$(printf '%s' "$name" | cut -c3-4)/$name"
fi

url="https://index.crates.io/$path"

for _ in $(seq 1 30); do
    if curl -fsS "$url" 2>/dev/null | grep -q "\"vers\":\"$version\""; then
        echo "$name@$version is indexed"
        exit 0
    fi
    sleep 10
done

echo "error: $name@$version did not appear in the crates.io index after 5 minutes" >&2
exit 1
