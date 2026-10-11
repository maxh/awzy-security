#!/usr/bin/env bash
# The linker rustc runs for linux/arm64 binaries (#93): Zig's, minus -Wl,--fix-cortex-a53-843419,
# which rustc's aarch64-unknown-linux-gnu target always passes and Zig's linker rejects. Graviton
# cores don't have the Cortex-A53 erratum it works around.
args=()
for arg in "$@"; do
  [ "$arg" = "-Wl,--fix-cortex-a53-843419" ] || args+=("$arg")
done
exec external/hermetic_cc_toolchain++toolchains+zig_config/tools/aarch64-linux-gnu.2.31/c++ "${args[@]}"
