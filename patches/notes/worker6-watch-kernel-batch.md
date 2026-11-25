# Worker 6 — write-watch kernel attribution and batching

Candidate: [worker6-watch-kernel-batch.patch.txt](worker6-watch-kernel-batch.patch.txt), staged in 49273718903e92af2580c278f9ae8400e57c2bd3 on main, after batch 1. Source preimage watch.rs blob 98e1c5702df5a29f0d68fa2d59fd1eecb7849e5e. No other source or mapping API touched.

## Why this candidate

The process PageFaultCount is not the number of guard exceptions. Historical same-frame lines show 118,734 process faults vs 212 guard exceptions and 21.64 ms successful watch/guard time; the current 80–94k and ~460 ms aggregate kernel CPU need attribution, not an assumed 460 ms saving. ask_kernel() used to return early on failure before accounting for the query; repeated speculative asks for ranges overlapping guarded views can therefore be expensive without appearing in watches ms. protect() tries a whole run, then retries each 64 KiB block and queries each failed block even if the first query established a long contiguous uncommitted region.

* COMET_GUARD_AHEAD_SKIP=1: skip only speculative GetWriteWatch requests overlapping tracked protected blocks. Return false, never remember the skipped span as covered; existing split/fallback remains. COMET_GUARD_ASK=1 overrides this skip.
* COMET_GUARD_BATCH=1: after the existing failed per-block protect and successful uncommitted-region query, mark the remaining whole blocks in that hole absent in one run, clipped to the requested end. Subsequent guards still query absent regions because ordinary commits may arrive without a remap. No successful protection or dirty journal write is skipped. Both optimization switches default off and can be evaluated independently.
* COMET_WATCH_TRACE=1 in a diagnostics build: logs "watch kernel interval (calls, us)" for ahead (including failures and skips), direct GetWriteWatch, protect, open, view, holes and writable queries. Time is summed elapsed microseconds, not process kernel CPU or fault attribution. Counts are raw per counter-drain interval, not per frame; sum and divide by corresponding presents over the same window. No extra process counter syscalls per watch call. Trace off for final FPS.

## Gate

```powershell
git apply --check docs/worker6-watch-kernel-batch.patch.txt
git apply docs/worker6-watch-kernel-batch.patch.txt
cargo build -p comet
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
$env:COMET_GUARD_AHEAD_SKIP="1"
$env:COMET_GUARD_BATCH="1"
cargo test -p comet-gpu --lib --locked watch::tests::
cargo build --profile diag -p comet --features diagnostics
```

Run the watch tests with switches unset as well; exercise clean_view_fast_path_still_observes_cpu_writes, writes_in_partly_committed_view_blocks_are_reported, unmapped_blocks_are_reported_once_until_mapped and kernel_reads_into_guarded_blocks_after_opening. New pure range test covers protected overlap endpoints.

Same current binary, warmed identical scene, Relay and other flags constant. First trace baseline (both optimization flags 0), then only AHEAD_SKIP=1, then only BATCH=1. Record the new interval line, copies per frame including page faults/guard faults/watches ms/unchanged MB, guards ms, process kernel time, copy bytes, prepare/P95 and correctness. AHEAD_SKIP should reduce ahead actual calls/failures and summed ahead us. BATCH should reduce protect and holes calls/us where holes span multiple blocks. Do not claim savings in process page faults or 460 ms kernel time without measured changes. If these categories are small, collect Windows ETW page-fault classes and CPU stacks to distinguish snapshot/working set/copy/remap costs and send evidence to owners. Keep trace enabled in all three diagnostic runs, disabled in throughput comparison. If no measurable benefit leave both switches off.

Static verification only: all 10 unified diff hunks replay against current source, mapping lock region unchanged, 885,735 stable mapping/hole model cases preserve dirty/absent/protected outcomes and never increase modeled fallback syscall count. No Rust toolchain/Windows here; Windows build gate must run before source integration. No game assets or source changes pushed.


## Batch 3 independent reduction — c5317cf

[worker6-guard-direct-skip.patch.txt](worker6-guard-direct-skip.patch.txt), staged in [9e548f3](https://github.com/Fyntra-Development/Comet/commit/9e548f39d86fc22328917e5340314cc8b97b8780). Applies independently to c5317cf (watch.rs blob 98e1c5702df5a29f0d68fa2d59fd1eecb7849e5e) or with the prior trace/batch patch in either order (static context replay verified).

**COMET_GUARD_DIRECT_SKIP=1** (default off): writes() currently avoids GetWriteWatch only for entirely clean guarded views. A dirty view with a protected first block still attempts GetWriteWatch although a mapped section is not a write-watched reservation. The candidate reuses the existing guard scan and journal record under GUARDS instead of attempting that kernel query. First-block identity and scan share one mutex acquisition; uncommitted/absent alone is not accepted. Unknown, unguarded, empty and COMET_GUARD_ASK=1 ranges retain the previous path. No new locking order or change to mapping functions. A guard-only watch is still counted in Watches/WatchUs, so those counters are comparable. The existing regression clean_view_fast_path_still_observes_cpu_writes checks the dirty second block while first remains protected; added explicit assertion to guarantee the condition.

**Primary metric:** with COMET_WATCH_TRACE=1, the direct=(calls, us) tuple in "watch kernel interval" should decrease (one fewer direct GetWriteWatch per eligible dirty-view lookup). Compare process kernel CPU ms/frame, copies per frame page faults, watches ms, guard faults, copied/unchanged MB, prepare/P95 and visual correctness. Do not expect the raw watches count to fall (guard scanning remains); no quantified page-fault saving without A/B. Set COMET_GUARD_AHEAD_SKIP=0 and COMET_GUARD_BATCH=0 for isolation; baseline DIRECT_SKIP=0 vs 1, trace on for diagnostic comparisons and off for throughput runs. Same warmed scene seconds 20–140, same Relay setting (off for visual correctness baseline).

Build gate: apply --check then apply the new patch; both cargo clippy modes, cargo build -p comet, and cargo test -p comet-gpu --lib --locked watch::tests:: with DIRECT_SKIP=0 and 1 in separate processes; specifically run the clean-view regression, partly committed view, kernel read and unmapped-block tests. Re-run with COMET_GUARD_ASK=1 to check rollback. Seven hunks and both application orders statically verified. No Windows build/test or performance measurement by W6; build gate pending.
