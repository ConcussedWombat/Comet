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
