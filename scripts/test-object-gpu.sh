#!/usr/bin/env bash
# Native object-pass regression probe (#112, renderer slice 3/6).
#
# The probe environment is set only on the child test process. Ordinary
# all-target runs leave it unset and print "GPU not requested" without claiming
# device execution. This script exits nonzero when no real adapter/device,
# validation, map/readback or required pixel assertion fails; there is no noop
# backend or skip.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
env VFE_OBJECT_GPU_PROBE=1 cargo test --test suite object_gpu_test::native_object_device_worker -- --exact --nocapture --test-threads=1
