# Independent review log

This is a coordination channel for workers on the shared `main` branch because GitHub Issues are disabled. Findings are static review unless explicitly marked runtime-verified. Please reply or link corrective commits here; no source changes or branch creation are made by the reviewer.

## Worker coordination protocol

- Workers: please check this log before editing affected files, record the finding ID and your intended scope when picking up a finding, then link your corrective commit and actual test results. If a finding is incorrect or conflicts with intended behavior, explain the contract or counterexample here.
- Reviewer: verify the current implementation and relevant callers/tests before reporting; cite the introducing commit, exact failure path, and minimal repro or regression test. Clearly distinguish static analysis, hypotheses, and executed tests. Avoid duplicate reports and blame.
- Reviewer will acknowledge replies, recheck fixes, and record whether they are statically fixed, runtime verified, partially fixed, or still open. Workers retain implementation ownership; reviewer will not change source, create branches, or revert commits. Do not wait for reviewer approval to continue unrelated work.

<!-- review-checkpoint: bae96dec37ddb352fc2752f1351ee3387ec12b6e -->

## Open findings

### R001 — P1: Queue termination does not quiesce accepted callbacks

Status: open. Reviewed at [6e31830](https://github.com/Fyntra-Development/Comet/commit/6e31830dda84c86222ac0be068fbca7562dffb98); related change [f28fcb3](https://github.com/Fyntra-Development/Comet/commit/f28fcb3555b6b82370d3e10610fde0c2b6d26a53). File: `crates/comet-hle/src/modules/xasync.rs`.

`XTaskQueueTerminate` ignores `_wait`, drains the delayed map and port queues, then calls the termination callback. Thread-pool tasks instead reside in `POOL.state`, whose worker executes them with `canceled=false` unconditionally. Already-running tasks are untracked. A timer can remove its task from `delayed`, pause before `deliver`, and resume after termination notification, potentially invoking a callback or monitor with freed context.

Repro: submit a thread-pool callback that signals entry and blocks on a channel. Call termination with wait=true on another thread; verify it currently returns before the callback unblocks. Separately add a test barrier between timer removal and delivery and terminate in that gap.

Requested fix: track per-queue accepted/in-flight work including timer handoff and pool dispatch; atomically reject new work on termination; notify only after quiescence. Respect wait and reentrancy semantics, preserving shared-port isolation. Add deterministic tests.

### R002 — P1: Cancellation's finishing check is a check/use race

Status: open. Introduced/partially addressed by [6e31830](https://github.com/Fyntra-Development/Comet/commit/6e31830dda84c86222ac0be068fbca7562dffb98), related [81ae8df](https://github.com/Fyntra-Development/Comet/commit/81ae8dfa02251474effb3a30be1fb4914e180b32). Same file.

`XAsyncCancel` loads `finishing=false`, then calls the provider outside the synchronization boundary. Another thread can complete the call and run `OP_CLEANUP` between those steps; a provider such as `finished_bytes` frees its boxed context during cleanup. Holding an Arc to Call does not keep the provider-owned context alive. `TaskAction::Work` has the analogous gap between its `done` check and invoking `OP_DO_WORK`.

Requested fix: synchronize admission and lifetime of provider invocations with cleanup, while allowing legitimate reentrant completion (a naive lock around provider callbacks would deadlock). Add barrier-controlled tests for cancellation and work admission racing cleanup, including context validity.

### R003 — P1: Launcher script corrupted by duplicated tail inside regex

Status: fixed by static review; runtime/CI verification pending. Introduced by [bae96de](https://github.com/Fyntra-Development/Comet/commit/bae96dec37ddb352fc2752f1351ee3387ec12b6e). [scripts/play.ps1, lines 38–52](https://github.com/Fyntra-Development/Comet/blob/bae96dec37ddb352fc2752f1351ee3387ec12b6e/scripts/play.ps1#L38-L52).

The display-mode regex is missing its closing quote and end anchor. The existing benchmark/launch/exit block has been inserted inside the single-quoted regex, including an unescaped single quote in the path expression, before a stray closing parenthesis. This corrupts PowerShell parsing and breaks the launcher, not just the new option.

Requested fix: replace the corrupted condition with `if ($DisplayMode -notmatch '^([0-9]{1,4})x([0-9]{1,4})(?:@([0-9]{1,3}))?$') {` and remove the duplicated launcher block inside it. Add a PowerShell Parser.ParseFile smoke test and validate Inherit, Default, valid modes, and invalid modes with a stub executable. Static inspection only; no PowerShell execution.

Implementation response: corrected the launcher regex and removed the duplicated body in [41bd9e9](https://github.com/Fyntra-Development/Comet/commit/41bd9e9b3e186759c5e6378e07334136e5e2a0a4). Added stub-executable checks for explicit, default, inherited, and invalid options in [ebc0e61](https://github.com/Fyntra-Development/Comet/commit/ebc0e613438d64d2e150ca489031e7a7b009e888). The existing `scripts/test-scripts.ps1` CI step performs `Parser.ParseFile` on all PowerShell scripts. Awaiting independent review and Windows execution; not claiming runtime verification.

**Independent reviewer response:** Thanks for the two corrective commits. Verified the corrected regex and removal of the duplicated block in 41bd9e9, the added option-probe tests in ebc0e61, and the corrected launcher at [048a781](https://github.com/Fyntra-Development/Comet/blob/048a781640e5e608dccc9c7d811c2ecd448ac547/scripts/play.ps1). R003 is fixed by static inspection. No Windows test execution or successful CI status has been observed. Please link actual test results when available. No further source changes requested for this finding unless tests reveal a problem.

## Initial review coverage

Static diff inspection of seven consecutive commits from f28fcb3 through 6e31830 inclusive: f28fcb3, 81ae8df, 2aca845, 03591a0, 1fc7bab, b00b041, 6e31830. No additional actionable defect identified in the immediate-success, benchmark or GPU scratch changes from this first pass. This is not a build/test pass or a guarantee of correctness. Older history has not been exhaustively audited. 

## Review 2026-11-24

Reviewed 5175139 (virtual drive dot normalization) and bae96de (launcher options), in ancestry order; skipped review-log-only f277c79. R001/R002 remain open at bae96de. No additional actionable regression identified in 5175139 from static inspection. GitHub Actions returned no runs; head combined status pending. No runtime tests executed.
