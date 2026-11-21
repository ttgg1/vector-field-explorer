#!/usr/bin/env bash
# Isolated real GPU proof: never alter display variables inside the shared suite.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
env -u DISPLAY -u WAYLAND_DISPLAY VFE_GPU_NO_ADAPTER=1 WGPU_BACKEND=noop \
    cargo test --test suite cli_gpu_test::no_adapter_worker -- --exact --nocapture
env -u DISPLAY -u WAYLAND_DISPLAY VFE_GPU_RUNTIME=1 RUST_LOG=info \
    cargo test --test suite cli_gpu_test::displayless_gpu_worker -- --exact --nocapture
# Recurring #192 hardware proof: the CLI PNG path composites objects.
env -u DISPLAY -u WAYLAND_DISPLAY VFE_OBJECT_GPU_PROBE=1 RUST_LOG=info \
    cargo test --test suite cli_gpu_test::cli_png_object_device_probe -- --exact --nocapture

# Software availability is separate from successful real hardware proof.
lavapipe=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json
if [[ -f "$lavapipe" ]]; then
    env -u DISPLAY -u WAYLAND_DISPLAY VFE_GPU_RUNTIME=1 RUST_LOG=info \
        WGPU_BACKEND=vulkan VK_DRIVER_FILES="$lavapipe" \
        cargo test --test suite cli_gpu_test::displayless_gpu_worker -- --exact --nocapture
    env -u DISPLAY -u WAYLAND_DISPLAY VFE_OBJECT_GPU_PROBE=1 RUST_LOG=info \
        WGPU_BACKEND=vulkan VK_DRIVER_FILES="$lavapipe" \
        cargo test --test suite cli_gpu_test::cli_png_object_device_probe -- --exact --nocapture
else
    echo 'GPU runtime: software Vulkan (lavapipe) UNAVAILABLE; not tested' >&2
fi
