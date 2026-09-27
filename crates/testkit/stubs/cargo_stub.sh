#!/bin/sh
# A `cargo` for a test's PATH that answers every command with success and
# does nothing else: for a test about what a workflow does around a lint
# or a build, not about what the toolchain finds in the code.
exit 0
