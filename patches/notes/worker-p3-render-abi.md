# P3 render statistics: local ABI verification and validation

Public reference: [Microsoft Dynamic Power States](https://learn.microsoft.com/en-us/xbox/sustainability/developer-tools/dynamic-power-states) documents the Render payload and identifies GPUBusyDuration as the dynamic-resolution input. [FramePacing sample](https://github.com/microsoft/Xbox-GDK-Samples/tree/main/Samples/Graphics/FramePacing) documents token tagging and queries the enclosing D3D12XBOX_FRAME_STATISTICS array. Neither public page establishes the complete binary record, type mask, units of both render timestamps, or the target title's requested mask.

## Local ABI probe (do not commit SDK headers or title code)

Use a legally available matching Xbox Series GDK header or independently verify the constants from the local caller. The following original helper extracts only interoperability constants from that header. Build and run it locally with the matching x64 GDK include settings (the FramePacing sample includes gxdk.h and d3d12_xs.h). The first argument is the **independently confirmed** duration frequency in units/second, or 0 only if the duration uses QPC ticks. This helper does not infer the time units. Also confirm GPUWriteCompleteTime is in the QPC domain. If the target ABI differs, do not enable the candidate.

```cpp
#include <Windows.h>
#include <gxdk.h>
#include <d3d12_xs.h>
#include <cstddef>
#include <cstdio>
#include <cstdlib>

static_assert(sizeof(D3D12XBOX_RENDER_STATISTICS) == 24);
static_assert(offsetof(D3D12XBOX_RENDER_STATISTICS, PlaneIndex) == 0);
static_assert(offsetof(D3D12XBOX_RENDER_STATISTICS, GPUWriteCompleteTime) == 8);
static_assert(offsetof(D3D12XBOX_RENDER_STATISTICS, GPUBusyDuration) == 16);
static_assert(sizeof(decltype(D3D12XBOX_RENDER_STATISTICS::PlaneIndex)) == 1);
static_assert(sizeof(decltype(D3D12XBOX_RENDER_STATISTICS::GPUWriteCompleteTime)) == 8);
static_assert(sizeof(decltype(D3D12XBOX_RENDER_STATISTICS::GPUBusyDuration)) == 8);
static_assert(sizeof(decltype(D3D12XBOX_FRAME_STATISTICS::Type)) == 4);
int main(int argc, char** argv) {
    if (argc != 2 || argv[1][0] == '\0') return 2;
    for (const char* p = argv[1]; *p; ++p)
        if (*p < '0' || *p > '9') return 2;
    std::printf("%u,%zu,%zu,%zu,%llu\n",
        static_cast<unsigned>(D3D12XBOX_FRAME_STATISTICS_TYPE_RENDER),
        sizeof(D3D12XBOX_FRAME_STATISTICS),
        offsetof(D3D12XBOX_FRAME_STATISTICS, Type),
        offsetof(D3D12XBOX_FRAME_STATISTICS, Render),
        std::strtoull(argv[1], nullptr, 10));
}
```

Set COMET_P3_RENDER_ABI to the one line emitted by the helper after confirming it matches the actual caller ABI and the trace mask is Render alone. Do not guess or copy the synthetic layout used in the Rust unit test. If no matching header exists, inspect locally and report only the verified mask, stride, Type and Render offsets, member offsets/widths, timestamp domains, caller count/availability semantics, and whether masks are combined; no game-derived bytes/disassembly in the repository.

## Candidate scope

The candidate returns a single Render record for an exact Render-only request with nonzero capacity, only when a single-plane PresentX has succeeded for the token and every tracked tagged piece has finished. Other masks, invalid/evicted/incomplete tokens, and empty output return S_FALSE/count zero. No layout configured = original behavior. Confirm the caller's return/count conventions before enabling. It does not implement the other union alternatives. Never treat the synthetic unit-test layout as an actual ABI.

The backend measures genuine PC GPU timestamp spans around each waited ExecuteCommandListsX piece, using a reused query heap, readback and two marker command lists under the host submission lock. It retains token ownership through held/cut/split work, excludes the host compare tail, and converts the latest completed GPU timestamp through host queue clock calibration into QPC ticks. Sums measured per-piece elapsed GPU spans in the verified duration units. Ordinary untagged submissions and the host present copy are not counted. This is **host translated-work elapsed feedback**, not exact Xbox P4-normalized hardware busy cycles or the exact moment the plane's last pixel was written; possible small GPU scheduling gaps and marker overhead. Two extra queue ExecuteCommandLists calls per measured piece (and one extra if a compare tail is separated), but no extra fence waits. No Relay/late support in this first candidate; never change their configuration implicitly.

## Windows gate / benchmark

1. Review source and run `cargo test -p comet-gpu p3_render_abi_and_availability --locked`, both CI builds and Clippy modes, workspace tests. Use synthetic data only. V2 review token ownership, ledger lock ordering, query/resource lifetime, fence proof; V1-V5 second review API/type safety.
2. Run a diagnostic capture with the landed COMET_P3_FRAME_TRACE=1, inspect the actual request mask, capacity and token history. Confirm title uses ExecuteCommandListsX tagged submissions. Do not enable until the ABI and units are verified; combined request requires another implementation rather than partial invented records.
3. For an experimental real-feedback run, set COMET_P3_RENDER_STATS=1, COMET_P3_RENDER_ABI to the verified values, COMET_LATE_WAIT=0, COMET_RELAY=0, remove COMET_GPU_TIMING. With COMET_P3_FRAME_TRACE=1, expect P3 verified render ABI, P3 sealed render statistics (valid=true) and P3 render statistics returned. Verify nonzero times in correct units; compare against actual frame durations and GPU completion trace. Reject any missing/timed-out tokens or implausible values.
4. Separate clean same-scene A/B: both arms explicit LATE_WAIT=0, RELAY=0, same warmed build/cache and display mode, stats off vs on. Turn off verbose trace for FPS. Inspect primary depth viewport dimensions in a separate trace; lower internal viewport and stable image quality must be demonstrated. No forced resize or resolution override from this patch. Retest COMET_DISPLAY_MODE=1920x1080@60 independently.
5. If no request matches Render alone or the title uses untagged GPU work for its scaler, report the verified mask and submission path. Do not call the task complete until the requested path is supported and the build gate lands it. No game-derived data, binaries, or SDK headers belong in Git.
