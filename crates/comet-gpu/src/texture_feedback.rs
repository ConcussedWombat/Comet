//! M9 positive direct-heap usage feedback; no eviction or promotion.
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use windows::Win32::Graphics::Direct3D12::*;
use crate::descriptor::host::TextureQueue;
use crate::host::{HostList, host, transition};

const BYTES: u64 = comet_shader::emit::TEXTURE_FEEDBACK_BYTES;
const SLOTS: u32 = comet_shader::emit::TEXTURE_FEEDBACK_SLOTS;
const RING: usize = 3;
static HEAPS: Mutex<Vec<Weak<Feedback>>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Host opt-in, independent of the translator's thread-local capability.
pub(crate) fn requested() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_M9_TEXTURE_FEEDBACK").is_some_and(|v| v == "1"))
}

/// Only the host half requests this translation variant.
pub(crate) fn translation_supported() -> bool {
    requested() && host().is_some()
}

/// If a title has a real root at the reserved slot, translator falls
/// back to plain DXIL; never hijack that root on the host side.
pub(crate) fn internal_slot(slot: u8, translated: &crate::pipeline::Translated) -> bool {
    requested()
        && slot == comet_shader::emit::SLOT_TEXTURE_FEEDBACK
        && translated.host.root_uavs.contains(&slot)
        && !translated.used.iter().any(|u| u.slot == slot)
}

/// OR-able nibble: use=1, requested view mip0=2, mip1=4, lower=8.
/// All classes may mean unknown LOD. Queued entries carry start tokens.
pub(crate) struct Snapshot {
    pub(crate) start: u64,
    pub(crate) frame: u64,
    pub(crate) slots: Vec<(u32, u8)>,
    pub(crate) queued: Vec<(u32, u64, u8)>,
}
impl Snapshot {
    #[allow(dead_code)] // N5 residency consumer follows separately.
    pub(crate) fn mip_classes(&self, index: u32) -> u8 {
        self.slots.binary_search_by_key(&index, |&(i, _)| i).ok()
            .map_or(0, |i| self.slots[i].1 & 14)
    }
}
struct Epoch {
    start: u64,
    frame: u64,
    tokens: Vec<(u32, u64)>,
}
struct Readback {
    buffer: ID3D12Resource,
    pending: Option<(Epoch, u64)>,
}
struct State {
    epoch: Option<(u64, Vec<(u32, u64)>)>,
    ring: Vec<Readback>,
    latest: Option<Arc<Snapshot>>,
}
pub(crate) struct Feedback {
    id: u64,
    count: u32,
    total: u32,
    queue: Arc<Mutex<TextureQueue>>,
    gpu: ID3D12Resource,
    zero: ID3D12Resource,
    state: Mutex<State>,
}

fn decode(words: &[u32], count: u32, total: u32) -> (Vec<(u32, u8)>, [u32; 3]) {
    let mut direct = Vec::new();
    let mut other = [0u32; 3]; // dynamic, outside, sink
    for (word, &bits) in words.iter().enumerate() {
        let mut bits = bits;
        while bits != 0 {
            let nibble = bits.trailing_zeros() / 4;
            let value = ((bits >> (nibble * 4)) & 15) as u8;
            bits &= !(15 << (nibble * 4));
            if value & 1 == 0 { continue; }
            let index = word as u32 * 8 + nibble;
            if index >= SLOTS - 1 { other[2] += 1; }
            else if index < count { direct.push((index, value)); }
            else if index < total { other[0] += 1; }
            else { other[1] += 1; }
        }
    }
    (direct, other)
}
fn join(slots: &[(u32, u8)], tokens: &[(u32, u64)]) -> Vec<(u32, u64, u8)> {
    slots.iter().filter_map(|&(i, bits)| {
        let pos = tokens.binary_search_by_key(&i, |&(index, _)| index).ok()?;
        Some((i, tokens[pos].1, bits))
    }).collect()
}

impl Feedback {
    pub(crate) fn new(count: u32, total: u32, queue: Arc<Mutex<TextureQueue>>) -> Option<Arc<Self>> {
        let host = host()?;
        let gpu = host.uav_buffer(BYTES)?;
        let (zero, data) = host.upload_buffer(BYTES)?;
        // SAFETY: fresh mapped upload, immutable thereafter.
        unsafe { data.write_bytes(0, BYTES as usize) };
        let mut ring = Vec::with_capacity(RING);
        for _ in 0..RING {
            ring.push(Readback { buffer: host.read_back_buffer(BYTES)?, pending: None });
        }
        let f = Arc::new(Self {
            id: NEXT.fetch_add(1, Relaxed), count, total, queue, gpu, zero,
            state: Mutex::new(State { epoch: None, ring, latest: None }),
        });
        HEAPS.lock().unwrap_or_else(|e| e.into_inner()).push(Arc::downgrade(&f));
        tracing::info!(heap = f.id, count, total, bytes = BYTES, "M9 GPU texture feedback heap allocated");
        Some(f)
    }
    pub(crate) fn bind(&self, list: &HostList) -> Option<u64> {
        list.keep(&self.gpu);
        // SAFETY: buffer held by list.
        Some(unsafe { self.gpu.GetGPUVirtualAddress() })
    }
    pub(crate) fn latest(&self, frame: u64) -> Option<Arc<Snapshot>> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.collect(&mut s, crate::host::texture_feedback_finished());
        s.latest.as_ref().filter(|f| f.start < f.frame && f.frame <= frame && frame - f.frame <= 2).cloned()
    }
    fn collect(&self, s: &mut State, finished: u64) {
        let mut ready: Vec<_> = s.ring.iter().enumerate().filter_map(|(i, r)| {
            let (epoch, flight) = r.pending.as_ref()?;
            (*flight <= finished).then_some((epoch.frame, i))
        }).collect();
        ready.sort_unstable();
        for (_, i) in ready {
            let r = &mut s.ring[i];
            let Some((epoch, _)) = r.pending.take() else { continue };
            let Some(host) = host() else { continue };
            // SAFETY: fence completed, exact mapped range.
            let (slots, other) = unsafe {
                if host.device.GetDeviceRemovedReason().is_err() { continue; }
                let mut data = std::ptr::null_mut();
                let range = D3D12_RANGE { Begin: 0, End: BYTES as usize };
                if r.buffer.Map(0, Some(&range), Some(&mut data)).is_err() { continue; }
                if data.is_null() { r.buffer.Unmap(0, None); continue; }
                let words = std::slice::from_raw_parts(data.cast::<u32>(), (BYTES / 4) as usize);
                let decoded = decode(words, self.count, self.total);
                let none = D3D12_RANGE { Begin: 0, End: 0 };
                r.buffer.Unmap(0, Some(&none));
                decoded
            };
            let queued = join(&slots, &epoch.tokens);
            let mip0 = slots.iter().filter(|(_, b)| b & 2 != 0).count();
            let mip1 = slots.iter().filter(|(_, b)| b & 4 != 0).count();
            let lower = slots.iter().filter(|(_, b)| b & 8 != 0).count();
            tracing::info!(heap = self.id, start = epoch.start, frame = epoch.frame,
                direct = slots.len(), dynamic = other[0], outside = other[1], sink = other[2],
                queued = queued.len(), mip0, mip1, lower, "M9 GPU texture feedback readback");
            if s.latest.as_ref().is_none_or(|old| old.frame <= epoch.frame) {
                s.latest = Some(Arc::new(Snapshot { start: epoch.start, frame: epoch.frame, slots, queued }));
            }
        }
    }
}

pub(crate) struct Ticket(Arc<Feedback>, usize, Epoch);

/// Present copy separately submitted to same direct queue; buffers decay
/// to COMMON at ExecuteCommandLists boundary. Discard first interval;
/// first reset establishes known zero. No CPU wait on ring saturation.
pub(crate) fn record(list: &HostList, frame: u64, finished: u64) -> Vec<Ticket> {
    if !requested() { return Vec::new(); }
    let mut registry = HEAPS.lock().unwrap_or_else(|e| e.into_inner());
    let live: Vec<_> = registry.iter().filter_map(Weak::upgrade).collect();
    registry.retain(|h| h.strong_count() != 0);
    drop(registry);
    let mut tickets = Vec::new();
    for f in live {
        let mut s = f.state.lock().unwrap_or_else(|e| e.into_inner());
        f.collect(&mut s, finished);
        let Some(i) = s.ring.iter().position(|r| r.pending.is_none()) else { continue };
        let before = s.epoch.take();
        let rb = &s.ring[i].buffer;
        list.keep(&f.gpu);
        list.keep(&f.zero);
        list.keep(rb);
        // SAFETY: open list, live buffers, exact byte ranges.
        unsafe {
            if before.is_some() {
                list.list.ResourceBarrier(&[transition(&f.gpu, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
                list.list.CopyBufferRegion(rb, 0, &f.gpu, 0, BYTES);
                list.list.ResourceBarrier(&[transition(&f.gpu, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COPY_DEST)]);
            } else {
                list.list.ResourceBarrier(&[transition(&f.gpu, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST)]);
            }
            list.list.CopyBufferRegion(&f.gpu, 0, &f.zero, 0, BYTES);
            list.list.ResourceBarrier(&[transition(&f.gpu, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COMMON)]);
        }
        let tokens = f.queue.lock().unwrap_or_else(|e| e.into_inner()).feedback_tokens();
        s.epoch = Some((frame, tokens));
        if let Some((start, tokens)) = before {
            tickets.push(Ticket(f.clone(), i, Epoch { start, frame, tokens }));
        }
    }
    tickets
}
pub(crate) fn submitted(tickets: Vec<Ticket>, flight: u64) {
    for Ticket(f, i, epoch) in tickets {
        f.state.lock().unwrap_or_else(|e| e.into_inner()).ring[i].pending = Some((epoch, flight));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn m9_decode_direct_dynamic_outside_and_sink() {
        let mut words = vec![0; (BYTES / 4) as usize];
        words[0] = 0x0009_5303; // slots 0=3, 2=3, 3=5, 4=9
        *words.last_mut().unwrap() = 0xf000_0000;
        let (slots, other) = decode(&words, 3, 4);
        assert_eq!(slots, vec![(0, 3), (2, 3)]);
        assert_eq!(other, [1, 1, 1]);
        assert_eq!(join(&slots, &[(0, 11), (1, 12)]), vec![(0, 11, 3)]);
        let snapshot = Snapshot { start: 0, frame: 1, slots, queued: vec![] };
        assert_eq!(snapshot.mip_classes(0), 2);
        assert_eq!(snapshot.mip_classes(1), 0);
    }
}
