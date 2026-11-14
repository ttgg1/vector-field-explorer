#!/usr/bin/env bash
# Real executable PNG decode/content and persistence proof in isolated children.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# Host implicit Vulkan layers can create their own config files; exclude them
# so this strict persistence check measures the application, not injected tools.
env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET VFE_CLI_RUNTIME=1 VK_LOADER_LAYERS_DISABLE='*' \
    cargo test --test suite cli_test::displayless_native_dispatch_worker -- --exact --nocapture
