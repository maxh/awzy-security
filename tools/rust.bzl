"""Macros that give every awzy Rust crate the same shape."""

load("@bazel_lib//lib:transitions.bzl", "platform_transition_filegroup")
load("@crates//:defs.bzl", "all_crate_deps")
load("@rules_oci//oci:defs.bzl", "oci_image", "oci_image_index", "oci_load", "oci_push")
load("@rules_pkg//pkg:tar.bzl", "pkg_tar")
load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_library", "rust_test")

# Where portal images put the built frontend.
FRONTEND_DIR = "/usr/share/awzy/portal"

# Every image is built for x86 and for Graviton (#93).
IMAGE_PLATFORMS = [
    "//tools/platforms:linux_amd64",
    "//tools/platforms:linux_arm64",
]

def awzy_rust_library(name, crate_name, deps = [], test_deps = [], compile_data = [], test_size = "small", visibility = None):
    """A library crate plus its unit tests (`:<name>_test`).

    Third-party deps come from the package's Cargo.toml via crate_universe;
    `deps` lists only first-party Bazel targets, and `test_deps` first-party targets that
    only the tests use (Cargo dev-dependencies). `test_size` is Bazel's: "small" allows a minute,
    "medium" five, for tests that run several services end to end.
    """
    rust_library(
        name = name,
        crate_name = crate_name,
        srcs = native.glob(["src/**/*.rs"], exclude = ["src/main.rs", "src/bin/**"]),
        deps = deps + all_crate_deps(normal = True),
        proc_macro_deps = all_crate_deps(proc_macro = True),
        compile_data = compile_data,
        visibility = visibility or ["//visibility:public"],
    )
    rust_test(
        name = name + "_test",
        crate = ":" + name,
        size = test_size,
        deps = test_deps + all_crate_deps(normal_dev = True),
        proc_macro_deps = all_crate_deps(proc_macro_dev = True),
    )

def awzy_service(name, crate_name, binary_name, deps = [], test_deps = [], compile_data = [], extra_binaries = [], image_binaries = [], frontend = None, test_size = "small", visibility = None):
    """A deployable service: library crate, unit tests, the `binary_name` executable, and its image.

    `extra_binaries` names files in src/bin/ (without .rs) to build as additional binaries.
    `image_binaries` names those of them the image ships too, beside the service.
    `frontend` is a directory target (a Vite `dist`) to ship in the image; the binary finds it
    through AWZY_PORTAL_STATIC_DIR.

    Image targets:
      :image_index  the release artifact: image index for IMAGE_PLATFORMS, buildable from macOS
      :image        single-platform image for --platforms (manual; built via :image_index)
      :image_load   loads into Docker as awzy/<binary>:dev (manual), e.g.
                    bazel run --platforms=//tools/platforms:linux_amd64 //authz/services/pdp:image_load
      :image_push   pushes :image_index (manual); CI passes the registry, e.g.
                    bazel run //authz/services/pdp:image_push -- --repository <registry>/awzy/awzy-pdp --tag <sha>
    """
    awzy_rust_library(
        name = name,
        crate_name = crate_name,
        deps = deps,
        test_deps = test_deps,
        compile_data = compile_data,
        test_size = test_size,
        visibility = visibility,
    )
    for binary in [binary_name] + extra_binaries:
        rust_binary(
            name = binary,
            srcs = ["src/main.rs"] if binary == binary_name else ["src/bin/%s.rs" % binary],
            deps = [":" + name] + deps + all_crate_deps(normal = True),
            proc_macro_deps = all_crate_deps(proc_macro = True),
            visibility = visibility or ["//visibility:public"],
        )

    pkg_tar(
        name = name + "_layer",
        srcs = [":" + b for b in [binary_name] + image_binaries],
        package_dir = "/usr/local/bin",
    )
    tars = [":" + name + "_layer"]
    env = {"AWZY_LISTEN_ADDR": "0.0.0.0:8080"}
    if frontend:
        # Static files are the same on every platform. Build them for the host, so the
        # bundler's native modules (Rolldown) match the machine running it.
        platform_transition_filegroup(
            name = name + "_frontend_host",
            srcs = [frontend],
            target_platform = "@platforms//host",
        )
        pkg_tar(
            name = name + "_frontend_layer",
            srcs = [":" + name + "_frontend_host"],
            package_dir = FRONTEND_DIR,
            # Drop the frontend's package path so files land at FRONTEND_DIR/dist/...
            strip_prefix = "/" + native.package_relative_label(frontend).package,
        )
        tars.append(":" + name + "_frontend_layer")
        env["AWZY_PORTAL_STATIC_DIR"] = FRONTEND_DIR + "/dist"
    oci_image(
        name = "image",
        base = "@distroless_cc",
        tars = tars,
        entrypoint = ["/usr/local/bin/" + binary_name],
        user = "nonroot",
        env = env,
        exposed_ports = ["8080/tcp"],
        tags = ["manual"],
    )
    oci_image_index(
        name = "image_index",
        images = [":image"],
        platforms = IMAGE_PLATFORMS,
        visibility = ["//visibility:public"],
    )
    oci_load(
        name = "image_load",
        image = ":image",
        repo_tags = ["awzy/%s:dev" % binary_name],
        tags = ["manual"],
    )
    oci_push(
        name = "image_push",
        image = ":image_index",
        repository = "awzy/%s" % binary_name,  # replaced at run time with --repository
        tags = ["manual"],
    )
