//! Bounded pilot of shared non-renderable texture heaps.
//! Adapted from Belobug/VIctory-Belobug-AMD 4cbe9d7
//! crates/victory-gpu/src/texpool.rs (GPL-3.0-or-later; VIctory by
//! Marvisak and contributors; fork changes by Belobug).
//! No slot reuse: descriptor/list COM ownership alone does not establish
//! alias-barrier or last GPU use. Retain heaps for the process lifetime.
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use windows::Win32::Graphics::Direct3D12::*;

const ALIGN: u64 = 4 << 10;
const HEAP_BYTES: u64 = 16 << 20;
const LIMIT: u64 = 256 << 20;
const MAX_SLOT: u64 = 32 << 10;

struct Slab {
    heap: ID3D12Heap,
    next: u64,
}

// SAFETY: the D3D12 heap is free-threaded; offsets are mutex-protected.
unsafe impl Send for Slab {}

static SLABS: Mutex<Vec<Slab>> = Mutex::new(Vec::new());
static PHYSICAL: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PLACED: AtomicU64 = AtomicU64::new(0);

pub(super) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R3_TEXTURE_POOL").is_some_and(|v| v == "1"))
}

fn slot(next: u64, bytes: u64, alignment: u64) -> Option<(u64, u64)> {
    if alignment != ALIGN || bytes == 0 || bytes > MAX_SLOT {
        return None;
    }
    let start = next.checked_add(alignment - 1)? & !(alignment - 1);
    let end = start.checked_add(bytes)?.checked_add(alignment - 1)? & !(alignment - 1);
    (end <= HEAP_BYTES).then_some((start, end))
}

pub(super) fn stranded_bytes() -> u64 {
    PHYSICAL.load(Relaxed).saturating_sub(LIVE.load(Relaxed))
}

pub(super) fn released(bytes: u64) {
    LIVE.fetch_sub(bytes, Relaxed);
}

pub(super) fn place(
    device: &ID3D12Device,
    mut desc: D3D12_RESOURCE_DESC,
    state: D3D12_RESOURCE_STATES,
) -> Option<(ID3D12Resource, u64)> {
    if desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
        || desc.Flags.0 != 0 || desc.SampleDesc.Count != 1
    {
        return None;
    }
    desc.Alignment = ALIGN;
    // SAFETY: a complete host resource description.
    let info = unsafe { device.GetResourceAllocationInfo(0, &[desc]) };
    slot(0, info.SizeInBytes, info.Alignment)?;
    let mut slabs = SLABS.lock().unwrap_or_else(|e| e.into_inner());
    let index = match slabs.iter().position(|s| slot(s.next, info.SizeInBytes, info.Alignment).is_some()) {
        Some(i) => i,
        None => {
            if PHYSICAL.load(Relaxed) >= LIMIT {
                return None;
            }
            let heap_desc = D3D12_HEAP_DESC {
                SizeInBytes: HEAP_BYTES,
                Properties: D3D12_HEAP_PROPERTIES {
                    Type: D3D12_HEAP_TYPE_DEFAULT,
                    ..Default::default()
                },
                Alignment: 64 << 10,
                Flags: D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES,
            };
            let mut made: Option<ID3D12Heap> = None;
            // SAFETY: a complete heap description and valid out pointer.
            unsafe { device.CreateHeap(&heap_desc, &mut made) }.ok()?;
            slabs.push(Slab { heap: made?, next: 0 });
            PHYSICAL.fetch_add(HEAP_BYTES, Relaxed);
            slabs.len() - 1
        }
    };
    let s = &mut slabs[index];
    let (start, end) = slot(s.next, info.SizeInBytes, info.Alignment)?;
    let mut texture: Option<ID3D12Resource> = None;
    // SAFETY: never-used aligned interval inside a retained heap.
    unsafe { device.CreatePlacedResource(&s.heap, start, &desc, state, None, &mut texture) }.ok()?;
    let texture = texture?;
    s.next = end;
    let bytes = end - start;
    LIVE.fetch_add(bytes, Relaxed);
    let n = PLACED.fetch_add(1, Relaxed) + 1;
    if n.is_power_of_two() {
        tracing::info!(n, heap_count = slabs.len(), physical = PHYSICAL.load(Relaxed),
            live = LIVE.load(Relaxed), stranded = stranded_bytes(), format = desc.Format.0,
            slot_bytes = bytes, "R3 pooled texture placed");
    }
    Some((texture, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r3_pool_slots_aligned_disjoint_and_bounded() {
        let mut next = 0;
        for bytes in [1, 4096, 4097, 16384, MAX_SLOT] {
            let (start, end) = slot(next, bytes, ALIGN).unwrap();
            assert!(start >= next && start.is_multiple_of(ALIGN));
            assert!(end >= start + bytes && end.is_multiple_of(ALIGN));
            next = end;
        }
        assert_eq!(slot(HEAP_BYTES - ALIGN, ALIGN, ALIGN), Some((HEAP_BYTES - ALIGN, HEAP_BYTES)));
        assert!(slot(HEAP_BYTES, 1, ALIGN).is_none());
        assert!(slot(u64::MAX, 1, ALIGN).is_none());
        assert!(slot(0, 1, 65536).is_none());
        assert!(slot(0, MAX_SLOT + 1, ALIGN).is_none());
        assert!(slot(0, 0, ALIGN).is_none());
    }
}
