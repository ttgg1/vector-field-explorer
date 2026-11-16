#!/usr/bin/env bash
# Native scalar-plot regression probe (#41, Task 8).
#
# The probe environment is set only on the child test process. Ordinary
# all-target runs leave it unset and print "GPU not requested" without claiming
# device execution. This script exits nonzero when no real adapter/device,
# validation, map/readback or required pixel assertion fails; there is no noop
# backend or skip.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
env VFE_SCALAR_GPU_PROBE=1 /usr/bin/cargo test --test suite plot_gpu_test::native_plot_device_worker -- --exact --nocapture --test-threads=1
