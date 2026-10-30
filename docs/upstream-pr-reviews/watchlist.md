# Upstream PRs on watch

Open upstream PRs we are not taking yet, but will take as soon as upstream signals they are good.

**Rule:** when upstream approves a watched PR for merge (an approving review) or merges it, treat it as vetted by
upstream and port it without a fresh full review. We still run the clean-room, brand, never-crash and asset checks
that every port gets, and test on GitHub only. A PR that stays open and unapproved stays here; one upstream closes
without merging leaves the list (record it in `closed-unmerged.md`). Move a PR's row to the matching review file once
it is ported.

Checked every 15 minutes while a release or merge round is running, otherwise at the weekly review
(`docs/upstream-merge.md`).

| PR | Title | Added | Upstream state when added |
|---|---|---|---|
| 3232 | Fix/nr shadow luminance bounds | 2026-10-30 | open, draft |
| 3298 | Issue #3272: AI masks are enlarged along the photo's edges, no outline under a raised Exposure | 2026-10-30 | open (reviewed r6: SKIP for now) |
| 3266 | LRC-DEV-TONAL: consolidate tonal processing as V3 | 2026-10-30 | open |
| 3279 | LR-EDIT-DETAIL-DENOISE: AI Denoise for finished pictures (JPEG, PNG, TIFF …) with a model of their own | 2026-10-30 | open |
| 3194 | LR-MASK-BRUSH: preserve Lightroom brush feathering and erasures | 2026-10-30 | open |
| 3231 | Issue #1337: restart instead of stopping when the window's GPU device is lost | 2026-10-30 | open, draft |

Check with:

```sh
gh pr view <n> --repo <upstream repo> --json state,isDraft,reviewDecision,mergedAt
```
