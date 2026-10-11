# TODO

Known work that isn't done yet. What each service does today is in the platform plan's
[Built so far](docs/platform-plan.md#built-so-far).

- linux/arm64 images: the Zig linker rejects rustc's `--fix-cortex-a53-843419` flag (see `tools/rust.bzl`).
- Optimized (`-c opt`) release builds.
- Real implementations behind the remaining service skeletons; the corp portal has no MCP tools yet.
