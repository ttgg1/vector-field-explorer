# Review record — issue #300: depth-test reference grid against solid objects

**Ticket:** 300

**Reviewed:** 2026-11-23
**Branch:** `feature/grid-solid-occlusion` (merged to main as `1952c03`)
**Product base:** `d98ad1337a265f0fc33b91ce609e3a6699debb84` (original), rebased onto
`93a4ede7673a38f12cbaad3d749ff3c5f632ed6a`
**Reviewed commit:** `2fe51149d8834238ce404d0959d6a68931de943e`

## Review disposition

The high-risk release review of the original `fd65815` diff returned **BLOCKED**
with two findings: unsupported depth-texture `textureLoad` on OpenGL/WebGL2 and
domain-box edges misclassified as face-grid lines subject to object occlusion.
A scoped fix at `f4accd3` closed both (portable `textureSampleLevel` depth read
with a NEAREST/non-comparison sampler; face-grid prefix tagging so box edges and
axes remain painter lines), and the branch was cleanly rebased onto `93a4ede` as
`2fe5114`. The reconciled fresh review returned **APPROVED** with both findings
closed, the `LineInstanceGPU` ABI unchanged, and no new defects. Diff SHA-256
`6a477f5b90cd75bea94c53e89c3396e55fe55649218584f6e3dc6a8926053bb4` was
byte-identical before and after the rebase. PR #302 merged the reviewed HEAD
non-squash into main.

## Evidence and limits

TDD RED/GREEN logs, native gates (2686 passed / 0 failed / 1 ignored, clippy,
docs, diff, smoke), and reconciled review evidence are archived under
`.superpowers/sdd/2026-11-18-physics-exploration/evidence/issue-300-grid-solid-occlusion/`.
Dedicated browser/manual visual checks remain owner-waived, not passed.
