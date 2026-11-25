//! Texture contents the title writes itself (decision 0007, "Texture
//! memory"): streamed pages, layout-library copies, movie frames, glyph
//! atlases. Such a texture lives in console memory in its tiled layout;
//! before a submission samples its host copy, the memory is detiled into a
//! staging buffer when its content changed, and the staging buffer is
//! copied into the host texture.
//!
//! "Changed" means a page of the memory was written since the last detiling
//! ([`crate::watch`]), in write-watched memory; in physical-page views,
//! which cannot be watched, a content hash of the whole memory at every
//! submission that samples it.
//!
//! shortcut: a change detiles the whole texture, not the pages written.
//! Only textures read through descriptor tables are brought up to date
//! (directly indexed views are not known per draw); plane 0 is uploaded
//! (every mip and array slice), depth and stencil planes are not.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use windows::Win32::Graphics::Direct3D12::*;

use crate::host::{HostList, host};
use crate::object::This;

mod cloth_probe;

macro_rules! once {
    ($($arg:tt)*) => {{
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| tracing::warn!($($arg)*));
    }};
}

/// A texture's upload state: the staging buffer laid out as the host
/// texture's subresources, and what was last detiled.
pub(crate) struct Staging {
    buffer: ID3D12Resource,
    /// Its bytes.
    #[cfg(feature = "diagnostics")]
    size: u64,
    data: *mut u8,
    footprints: Vec<D3D12_PLACED_SUBRESOURCE_FOOTPRINT>,
    rows: Vec<u32>,
    layout: comet_formats::texture::Layout,
    /// Mips per array slice (of the host copy).
    mips: u32,
    /// The title's most detailed levels the host copy leaves out
    /// (`crate::resource::mip_skip_for`): host level `m` is title level
    /// `m + skip`.
    skip: u32,
    /// The bytes of title memory read, from its start: all of it, or with
    /// levels left out only up to the largest one left out (a texture's
    /// smallest levels lie first, its largest last): the levels left out
    /// are not read, hashed or waited for.
    read_len: usize,
    /// The staging buffer's bytes (counted in `STAGED` while it lives).
    bytes: u64,
    memory: *const u8,
    last: Mutex<Last>,
    /// Whether the staging buffer was detiled since a submitted list last
    /// uploaded it, and whether one has uploaded it at all: a list records
    /// the upload only then. (Every list that sampled a title texture
    /// uploaded all of it again, changed or not: 5,411 subresource copies
    /// of 1,120 textures in one frame of run 18.)
    dirty: std::sync::atomic::AtomicBool,
    uploaded: std::sync::atomic::AtomicBool,
    /// When a list last recorded a use of it (`crate::host::moment`).
    used: std::sync::atomic::AtomicU64,
    /// Background partial upload retention, separate from table ownership.
    w3_background: std::sync::atomic::AtomicU8,
}

/// What a texture's staging buffer holds: the hash of the memory detiled
/// (memory not watched), or the write-watch asking it was detiled after.
#[derive(Default)]
struct Last {
    hash: Option<blake3::Hash>,
    asked: u64,
    /// The frame a partly mapped texture was last read in.
    partial: Option<u64>,
    /// Hashes of complete subresource spans from the partial BC path.
    resident_hashes: Vec<Option<blake3::Hash>>,
    /// Last count of readable subresources, for transition-only logging.
    resident_count: Option<usize>,
}

// SAFETY: the staging mapping is written only at submission, under
// `last`'s lock; the title memory is only read.
unsafe impl Send for Staging {}
// SAFETY: as above.
unsafe impl Sync for Staging {}

static STAGING: Mutex<Option<HashMap<usize, Arc<Staging>>>> = Mutex::new(None);

/// The buffer a texture the host writes is copied through into the texture
/// of a view the title placed in its memory in another format
/// ([`alias`]): every level of every layer as each of the two formats lays
/// it out for a copy (the same bytes: the texels are as wide), the written
/// texture's layout first.
struct Through {
    buffer: ID3D12Resource,
    parts: Vec<(
        D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
        D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    )>,
}

/// The most levels times layers [`alias`] copies (a cube of nine levels).
const ALIAS_PARTS: u32 = 54;

// SAFETY: a device-local buffer only lists record copies with.
unsafe impl Send for Through {}
// SAFETY: as above.
unsafe impl Sync for Through {}

/// The copy buffers of [`alias`], by the placed view's texture.
static THROUGH: Mutex<Option<HashMap<usize, Arc<Through>>>> = Mutex::new(None);

/// Brings `placed`, the texture of a view the title placed in the memory
/// of `source` in another format (`crate::resource::alias_of`), up to date
/// with what the host wrote to `source`: the texels of every level and
/// layer are copied as they are, through a buffer (the host copies between
/// two formats of one texel width in no other way). Recorded the first time
/// the list reads the view
/// and again whenever the list used `source` in another way since (wrote
/// it, or read it itself). Leaves `placed` in the copy destination state,
/// as [`upload`] does. False if the copy cannot be made: the caller fills
/// the texture from title memory, as before.
pub(crate) fn alias(list: &HostList, placed: This, source: This) -> bool {
    const TARGET_OR_UNORDERED: u32 = 0x4 | 0x8;
    const COPY_DEST: u32 = 0x400;
    const COPY_SOURCE: u32 = 0x800;
    let (Some(to), Some(from)) = (
        crate::resource::host_resource(placed),
        crate::resource::host_resource(source),
    ) else {
        return false;
    };
    let Some(host) = host() else { return false };
    let through = {
        let mut map = THROUGH.lock().unwrap_or_else(|e| e.into_inner());
        let map = map.get_or_insert_with(HashMap::new);
        match map.get(&(placed as usize)) {
            Some(through) => through.clone(),
            None => {
                // SAFETY: live textures.
                let (of_from, of_to) = unsafe { (from.GetDesc(), to.GetDesc()) };
                // Levels times layers (a volume's slices are one part).
                let count = |d: &D3D12_RESOURCE_DESC| {
                    let layers = if d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
                        1
                    } else {
                        u32::from(d.DepthOrArraySize)
                    };
                    u32::from(d.MipLevels) * layers
                };
                let parts = count(&of_from);
                if parts == 0 || parts > ALIAS_PARTS || parts != count(&of_to) {
                    return false;
                }
                let mut rows = [(); 2]
                    .map(|()| vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); parts as usize]);
                let mut bytes = [0u64; 2];
                // SAFETY: plain queries; the out pointers are ours and
                // hold `parts` layouts each.
                unsafe {
                    host.device.GetCopyableFootprints(
                        &of_from,
                        0,
                        parts,
                        0,
                        Some(rows[0].as_mut_ptr()),
                        None,
                        None,
                        Some(&mut bytes[0]),
                    );
                    host.device.GetCopyableFootprints(
                        &of_to,
                        0,
                        parts,
                        0,
                        Some(rows[1].as_mut_ptr()),
                        None,
                        None,
                        Some(&mut bytes[1]),
                    );
                }
                let alike = rows[0].iter().zip(&rows[1]).all(|(a, b)| {
                    let (fa, fb) = (a.Footprint, b.Footprint);
                    a.Offset == b.Offset
                        && (fa.Width, fa.Height, fa.Depth, fa.RowPitch)
                            == (fb.Width, fb.Height, fb.Depth, fb.RowPitch)
                });
                if bytes[0] == 0 || bytes[0] != bytes[1] || !alike {
                    return false;
                }
                let Some(buffer) = host.uav_buffer(bytes[0]) else {
                    return false;
                };
                crate::host::name(&buffer, || {
                    format!(
                        "alias copy {}x{} f{} to f{}",
                        of_from.Width, of_from.Height, of_from.Format.0, of_to.Format.0
                    )
                });
                tracing::info!(
                    view = ?placed,
                    texture = ?source,
                    width = of_from.Width,
                    height = of_from.Height,
                    parts,
                    written_as = of_from.Format.0,
                    read_as = of_to.Format.0,
                    "a view placed in the memory of a texture the host writes, in another format: read from that texture by a copy on the host"
                );
                let [of_written, of_read] = rows;
                let through = Arc::new(Through {
                    buffer,
                    parts: of_written.into_iter().zip(of_read).collect(),
                });
                map.insert(placed as usize, through.clone());
                through
            }
        }
    };
    let first = list.first_use(placed as usize);
    // The source's first use in the list records nothing (its prelude
    // brings it to the state); a use in another way since the last copy
    // shows as a change of state.
    let before = list.transition(source, COPY_SOURCE).map(|(texture, before)| {
        let state = crate::host::legal_state(texture, COPY_SOURCE);
        let barrier = crate::host::transition(
            texture,
            D3D12_RESOURCE_STATES(before as i32),
            D3D12_RESOURCE_STATES(state as i32),
        );
        // SAFETY: an open list and a live texture.
        unsafe { list.list.ResourceBarrier(&[barrier]) };
        before
    });
    if !first && before.is_none() {
        return true;
    }
    crate::list::ensure_state(list, placed, COPY_DEST);
    let part = |texture: &ID3D12Resource, index: usize| D3D12_TEXTURE_COPY_LOCATION {
        // SAFETY: a borrowed pointer the location never releases.
        pResource: unsafe { std::mem::transmute_copy(texture) },
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            SubresourceIndex: index as u32,
        },
    };
    let rows = |row: &D3D12_PLACED_SUBRESOURCE_FOOTPRINT| D3D12_TEXTURE_COPY_LOCATION {
        // SAFETY: as above.
        pResource: unsafe { std::mem::transmute_copy(&through.buffer) },
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: *row,
        },
    };
    let (common, read, written) = (
        D3D12_RESOURCE_STATE_COMMON,
        D3D12_RESOURCE_STATE_COPY_SOURCE,
        D3D12_RESOURCE_STATE_COPY_DEST,
    );
    #[cfg(feature = "diagnostics")]
    list.note_op(|| format!("alias copy of {source:?} into {placed:?}"));
    // SAFETY: an open list and live resources; the buffer is as big as
    // the footprints and rests in the common state between copies.
    unsafe {
        list.list
            .ResourceBarrier(&[crate::host::transition(&through.buffer, common, written)]);
        for (index, (of_written, _)) in through.parts.iter().enumerate() {
            list.list
                .CopyTextureRegion(&rows(of_written), 0, 0, 0, &part(from, index), None);
        }
        list.list
            .ResourceBarrier(&[crate::host::transition(&through.buffer, written, read)]);
        for (index, (_, of_read)) in through.parts.iter().enumerate() {
            list.list
                .CopyTextureRegion(&part(to, index), 0, 0, 0, &rows(of_read), None);
        }
        list.list
            .ResourceBarrier(&[crate::host::transition(&through.buffer, read, common)]);
    }
    list.keep(&through.buffer);
    // A draw that also writes the source (a view of it handled before
    // this one) needs it in that state again; the next read of the alias
    // then copies once more, as it must after a write.
    if let Some(before) = before
        && before & TARGET_OR_UNORDERED != 0
    {
        crate::list::ensure_state(list, source, before);
    }
    true
}

/// Whether a texture refresh that finds the runtime remapping title memory
/// goes on without waiting for it (`COMET_TEXTURE_NO_WAIT=1`; off by
/// default).
///
/// A texture whose memory is all mapped is read in place, under a lock
/// the runtime takes alone while it changes a mapping
/// (`crate::watch::title_memory_reading`), so that no page goes away under
/// the read. While the world streams in the runtime remaps all the time,
/// and each of the frame's 150-360 refreshes waited for it: runs 99-101
/// have 4, 15 and 16 frame lines with over 300 ms of "textures", up to
/// 3,708 ms, and on every one of them the wait for this lock is the whole
/// of it (1,345 of 1,381 ms; 3,630 of 3,708; the write-watch askings 3-12
/// ms, the texture's own lock 0).
///
/// With the switch the refresh asks for the lock without waiting. If the
/// runtime has it, the texture is read through the kernel instead, which
/// needs no lock: it fails if a page is not there, and then the texture is
/// left as it is and read at its next refresh, a frame later. A read
/// through the kernel costs about 0.6 ms a megabyte against 0.3 in place.
///
/// What changes for the picture: a texture that arrives while the runtime
/// is remapping may show a frame later. It never shows partly read.
fn no_wait() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_TEXTURE_NO_WAIT").is_some_and(|v| v == "1"))
}

/// Since the last frame-rate line, with [`no_wait`]: refreshes that read
/// through the kernel because title memory was being remapped, and those
/// left for the next refresh because a page was not there.
pub(crate) static UNLOCKED: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// Since the last frame-rate line, of texture refreshes on every thread, in
/// microseconds: waiting for the texture's own lock (another thread is
/// refreshing the same texture), asking the write watch and the guards,
/// waiting for title memory to hold still (the lock the runtime takes
/// alone while it remaps, `crate::watch::title_memory_reading`), and the
/// refreshes in all.
///
/// While the world streams in, the frame line's "textures" is seconds a
/// frame in some runs and tenths in others for the same megabytes (2,566
/// ms for 110 MB in run 93, 314 ms for 132 MB in run 91; before these
/// counters, runs 81, 83, 85 and 94 each had lines of 1.5-2.9 s), and the
/// submitting thread is not running while it lasts (run 93: 240-310 ms of
/// its own time a frame beside 1,100-1,800 ms of "textures"): it waits.
/// These say for what.
pub(crate) static REFRESH_WAITS: [std::sync::atomic::AtomicU64; 4] = [const { std::sync::atomic::AtomicU64::new(0) }; 4];

/// `f`'s result, its time added to [`REFRESH_WAITS`]' place `slot`.
fn waited<T>(slot: usize, f: impl FnOnce() -> T) -> T {
    let started = std::time::Instant::now();
    let out = f();
    REFRESH_WAITS[slot].fetch_add(started.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    out
}

thread_local! {
    /// Room to read a texture's memory into: one buffer per submitting
    /// thread. Each texture kept its own, a second copy in memory of every
    /// write-watched or partly mapped texture ever sampled, for the run.
    static READ: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Calls `f` with `size` bytes of this thread's read buffer (their
/// contents are whatever the last texture left).
fn with_read<R>(size: usize, f: impl FnOnce(&mut [u8]) -> R) -> R {
    READ.with(|read| {
        let mut read = read.borrow_mut();
        if read.len() < size {
            read.resize(size, 0);
        }
        f(&mut read[..size])
    })
}

/// Whether a texture all of whose memory is mapped is read through the
/// kernel's copy instead of in place (`COMET_TEXTURE_COPY=1`; off by
/// default). Round 3 of the memory work read every such texture that way
/// to be safe from pages taken away under the read; it cost 0.6 ms per MB
/// against 0.3 in place (90-105 ms a frame while the world streams in, runs
/// 72 and 73, against 20-40), and both runs with it lost the graphics
/// device, for a reason not found. The read in place is safe now for
/// another reason (`crate::watch::title_memory_reading`); this switch is
/// left to tell whether the copy was what lost the device.
fn through_copy() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_TEXTURE_COPY").is_some_and(|v| v == "1"))
}

/// Read the source while its mapping is stable; release the mapping
/// before passing the private copy to the expensive conversion.
fn with_snapshot<R, L>(
    size: usize,
    lock: L,
    copy: impl FnOnce(&mut [u8]),
    consume: impl FnOnce(&[u8]) -> R,
) -> R {
    with_read(size, |read| {
        copy(read);
        drop(lock);
        consume(read)
    })
}

/// Opt-in: don't turn missing pages into compressed texture blocks.
fn resident_bc() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R3_RESIDENT_BC").is_none_or(|v| v != "0"))
}

fn resident_bc_layout(layout: &comet_formats::texture::Layout) -> bool {
    resident_bc()
        && layout.desc.dimension == comet_formats::texture::Dimension::Tex2d
        && layout.desc.block == 4
        && layout.addressable()
}

/// Scratch contains arbitrary old bytes on a failed read. Only return
/// hashes for spans read successfully in full. Tail mips share a span;
/// do not reread it (a later failed read could spoil an earlier success).
fn read_resident_mips(
    layout: &comet_formats::texture::Layout,
    memory: *const u8,
    scratch: &mut [u8],
    count: usize,
    mips: u32,
    skip: u32,
) -> Vec<Option<blake3::Hash>> {
    let mut spans = HashMap::new();
    (0..count).map(|i| {
        let mip = i as u32 % mips + skip;
        let slice = i as u32 / mips;
        let span = layout.subresource_read_span(mip, slice)?;
        *spans.entry((span.start, span.end)).or_insert_with(|| {
            let dst = scratch.get_mut(span.clone())?;
            if crate::host::read_into(memory.wrapping_add(span.start), dst) {
                Some(blake3::hash(dst))
            } else {
                None
            }
        })
    }).collect()
}

/// Read through the kernel, including unmapped pages, for the layout tests.
#[cfg(test)]
fn with_title<R>(memory: *const u8, size: usize, f: impl FnOnce(&[u8]) -> R) -> R {
    with_read(size, |read| {
        crate::host::copy_mapped(read.as_mut_ptr(), memory, size, false);
        f(read)
    })
}

/// Copy a changed fully mapped texture to thread-local scratch before
/// detiling (`COMET_TEXTURE_SHORT_LOCK=1`). This trades one memory copy for
/// a shorter mapping read lock, so the streaming thread can remap while
/// we do the CPU conversion. Unchanged textures need no extra copy.
fn short_lock() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_TEXTURE_SHORT_LOCK").is_some_and(|v| v == "1"))
}

/// Staging buffers alive (also those only a list still holds) and their
/// bytes.
static STAGED: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

impl Drop for Staging {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering::Relaxed;
        STAGED[0].fetch_sub(1, Relaxed);
        STAGED[1].fetch_sub(self.bytes, Relaxed);
    }
}

/// For the memory line: staging buffers alive, their bytes, and how many
/// of them their texture still has (the others wait for a list's reset).
#[cfg(feature = "diagnostics")]
pub(crate) fn memory() -> [u64; 3] {
    use std::sync::atomic::Ordering::Relaxed;
    let kept = STAGING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or(0, |map| map.len() as u64);
    [STAGED[0].load(Relaxed), STAGED[1].load(Relaxed), kept]
}

/// Staging buffers let go as idle since the start, and their bytes.
static LET_GO: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// For the memory line: staging buffers let go as idle, and their bytes.
#[cfg(feature = "diagnostics")]
pub(crate) fn let_go() -> [u64; 2] {
    use std::sync::atomic::Ordering::Relaxed;
    [LET_GO[0].load(Relaxed), LET_GO[1].load(Relaxed)]
}

/// Lets go of the staging buffers no list recorded a use of for `IDLE_MS`
/// and `IDLE_FRAMES` frames and no list holds: a staging buffer is system memory the size of its
/// texture, kept so that an unchanged texture is not uploaded again, and
/// those of textures no longer sampled were kept for the run (2,246 of
/// them, 2.1 GB, at the end of run 24). The next list that samples such a
/// texture makes a staging buffer anew, detiles and uploads once more.
/// Called before a submission starts, by the thread that submits.
pub(crate) fn let_go_idle() {
    use std::sync::atomic::Ordering::Relaxed;
    const IDLE_MS: u64 = 10_000;
    const IDLE_FRAMES: u64 = 4;
    let mut map = STAGING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = map.as_mut() else {
        return;
    };
    // A list that recorded an upload or a check holds the staging until it
    // is reset: one only the table holds is named by no list, recorded or
    // running.
    let (mut went, mut bytes) = (0u64, 0u64);
    map.retain(|_, s| {
        let keep = Arc::strong_count(s) > 1
            || !crate::host::long_ago(s.used.load(Relaxed), IDLE_MS, IDLE_FRAMES);
        if !keep {
            LET_GO[0].fetch_add(1, Relaxed);
            LET_GO[1].fetch_add(s.bytes, Relaxed);
            went += 1;
            bytes += s.bytes;
        }
        keep
    });
    if went > 0 {
        crate::host::trail(|| format!("sweep: {went} staging buffers of textures let go ({} KiB), held by no list", bytes >> 10));
    }
}

/// Textures the host GPU wrote by a copy or resolve: their contents are on
/// the host, not in title memory, which stays as it was.
static HOST_WRITTEN: Mutex<Option<std::collections::HashSet<usize>>> = Mutex::new(None);

/// A host copy or resolve writes `resource`.
pub(crate) fn host_written(resource: This) {
    let mut set = HOST_WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    set.get_or_insert_with(Default::default)
        .insert(resource as usize);
}

/// Whether the host GPU wrote `resource` by a copy or resolve
/// ([`host_written`]): its host copy cannot be made again from title
/// memory.
pub(crate) fn host_wrote(resource: usize) -> bool {
    HOST_WRITTEN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|set| set.contains(&resource))
}

/// The title released a resource.
pub(crate) fn forget(resource: usize) {
    let mut map = STAGING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = map.as_mut() {
        map.remove(&resource);
    }
    let mut set = HOST_WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(set) = set.as_mut() {
        set.remove(&resource);
    }
    let mut through = THROUGH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(through) = through.as_mut() {
        through.remove(&resource);
    }
}

/// Retain partial BC staging through bindless retries, within the existing
/// idle sweep and a conservative total live staging budget.
fn w3_keep_partial() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_W3_KEEP_PARTIAL_STAGING").is_some_and(|v| v == "1"))
}

const W3_PARTIAL: u8 = 1;
const W3_RETAINED: u8 = 2;

/// Whether `resource`'s host copy takes its contents from title memory:
/// a texture the host GPU does not render to or write.
pub(crate) fn title_written(texture: &ID3D12Resource) -> bool {
    const TARGET_DEPTH_UNORDERED: i32 = 0x7;
    // SAFETY: a live resource.
    unsafe { texture.GetDesc() }.Flags.0 & TARGET_DEPTH_UNORDERED == 0
}

/// Records, once per list, the copy of `resource`'s staging buffer into
/// its host copy, and the detiling that refreshes the staging buffer at
/// submission if the memory changed. Leaves the texture in the copy
/// destination state.
pub(crate) fn upload(list: &HostList, resource: This, texture: &ID3D12Resource) {
    let host_written = HOST_WRITTEN
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|set| set.contains(&(resource as usize)));
    // shortcut: a texture the host copied into keeps the host's contents
    // even if the title later writes its memory too.
    if host_written || !list.first_use(resource as usize) {
        return;
    }
    let Some(staging) = staging(resource, texture) else {
        return;
    };
    use std::sync::atomic::Ordering::Relaxed;
    staging.used.store(crate::host::moment(), Relaxed);
    // A background retry must record a copy before its submission-time
    // refresh discovers newly resident mips. Ordinary table reads can
    // take ownership of the retained staging just as before.
    let retry = w3_keep_partial() && staging.w3_background.swap(0, Relaxed) == W3_RETAINED;
    // The host texture has the staging buffer's contents: only the check
    // at submission is left, and what it finds changed the next list that
    // samples the texture uploads (a frame late where every list of the
    // frame was recorded before that check).
    if !retry && staging.uploaded.load(Relaxed) && !staging.dirty.load(Relaxed) {
        #[cfg(feature = "diagnostics")]
        crate::copystat::add(crate::copystat::C::TexSkipped, 1);
        list.defer_texture(staging, false);
        return;
    }
    #[cfg(feature = "diagnostics")]
    crate::copystat::add(crate::copystat::C::TexUploads, 1);
    const COPY_DEST: u32 = 0x400;
    crate::list::ensure_state(list, resource, COPY_DEST);
    // The staging buffer lives as long as `staging`, which the deferred
    // refresh below holds until the list is reset.
    for (i, footprint) in staging.footprints.iter().enumerate() {
        #[cfg(feature = "diagnostics")]
        {
            let f = footprint.Footprint;
            // SAFETY: live resources.
            let (gpu, d) = unsafe { (staging.buffer.GetGPUVirtualAddress(), texture.GetDesc()) };
            let len = u64::from(f.RowPitch) * u64::from(staging.rows[i]) * u64::from(f.Depth);
            list.note_read(
                || {
                    format!(
                        "texture upload {resource:?} ({}x{}x{} f{} mips {}) sub {i} of {} from staging {gpu:#x}+{:#x}: offset {:#x} {}x{}x{} pitch {}",
                        d.Width,
                        d.Height,
                        d.DepthOrArraySize,
                        d.Format.0,
                        d.MipLevels,
                        staging.footprints.len(),
                        staging.size,
                        footprint.Offset,
                        f.Width,
                        f.Height,
                        f.Depth,
                        f.RowPitch
                    )
                },
                (gpu + footprint.Offset, len),
            );
        }
        let to = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: a borrowed pointer the location never releases.
            pResource: unsafe { std::mem::transmute_copy(texture) },
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: i as u32,
            },
        };
        let from = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: as above.
            pResource: unsafe { std::mem::transmute_copy(&staging.buffer) },
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: *footprint,
            },
        };
        // SAFETY: an open list; both locations name live resources.
        unsafe { list.list.CopyTextureRegion(&to, 0, 0, 0, &from, None) };
    }
    // When the list is submitted, its copy takes what the staging buffer
    // holds by then: noted before the check below, which sets `dirty`
    // again if it detiles (the next list then uploads once more).
    // Keep the upload mark and refresh together, in recorded order.
    // Once per submission however many of its lists sample the texture.
    list.defer_texture(staging, true);
}

/// Complete-snapshot experiment. R3 owns supported BC layouts when its
/// default resident-mip policy is active.
fn w3_full_snapshot(layout: &comet_formats::texture::Layout) -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_W3_TEXTURE_FULL_SNAPSHOT").is_some_and(|v| v == "1"))
        && !resident_bc_layout(layout)
}

/// A failed kernel read may modify scratch before returning. Do not
/// accept or hash its bytes as a complete source.
fn w3_complete_hash(read: &mut [u8], copy: impl FnOnce(&mut [u8]) -> bool) -> Option<blake3::Hash> {
    copy(read).then(|| blake3::hash(read))
}

impl Staging {
    /// A recorded upload will take the staging buffer's contents when
    /// the list runs. A subsequent refresh may make it dirty again.
    pub(crate) fn mark_uploaded(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        self.uploaded.store(true, Relaxed);
        self.dirty.store(false, Relaxed);
    }

    /// The title memory the texture is read from: address and bytes.
    pub(crate) fn memory(&self) -> (usize, usize) {
        (self.memory as usize, self.read_len)
    }

    /// Detiles the texture memory into the staging buffer if it changed
    /// since the last time (and is mapped at all).
    pub(crate) fn refresh(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let started = std::time::Instant::now();
        let read = self.refresh_now();
        let preparing = &crate::host::PREPARING;
        preparing[2].fetch_add(started.elapsed().as_micros() as u64, Relaxed);
        preparing[3].fetch_add(read as u64, Relaxed);
    }

    /// Returns the bytes of title memory read.
    fn refresh_now(&self) -> usize {
        let whole = std::time::Instant::now();
        let read = self.refresh_timed();
        REFRESH_WAITS[3].fetch_add(whole.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
        read
    }

    /// Deliberately bypass the watch and partial retry throttle for this
    /// correctness A/B. Only complete reads replace accepted staging bytes.
    fn refresh_full_snapshot(&self, last: &mut Last, size: usize) -> usize {
        let mapped = waited(2, || crate::watch::title_memory_reading_range(self.memory as usize, size));
        with_read(size, |read| {
            let hash = w3_complete_hash(read, |read| {
                let complete = crate::host::read_into(self.memory, read);
                drop(mapped);
                complete
            });
            let Some(hash) = hash else {
                if last.partial.is_none() {
                    tracing::info!(memory = ?self.memory, bytes = size,
                        dxgi = self.footprints[0].Footprint.Format.0,
                        "W3 texture snapshot rejected incomplete read");
                }
                last.partial = Some(crate::frame::current());
                last.asked = 0;
                return 0;
            };
            if last.partial.take().is_some() {
                tracing::info!(memory = ?self.memory, bytes = size,
                    dxgi = self.footprints[0].Footprint.Format.0,
                    "W3 texture snapshot recovered complete read");
            }
            #[cfg(feature = "diagnostics")]
            crate::copystat::add(crate::copystat::C::TexHashed, size as u64);
            if last.hash != Some(hash) {
                self.detile(read);
                last.hash = Some(hash);
                #[cfg(feature = "diagnostics")]
                crate::copystat::add(crate::copystat::C::TexChanged, size as u64);
            }
            size
        })
    }

    fn refresh_timed(&self) -> usize {
        let size = self.read_len;
        let start = self.memory as usize;
        let mut last = waited(0, || self.last.lock().unwrap_or_else(|e| e.into_inner()));
        let last = &mut *last;
        if w3_full_snapshot(&self.layout) {
            return self.refresh_full_snapshot(last, size);
        }
        if let Some((now, written)) = waited(1, || crate::watch::writes(start, start + size, false)) {
            if last.asked != 0 && written.iter().all(|&(_, w)| w <= last.asked) {
                return 0;
            }
            last.asked = now;
            if resident_bc_layout(&self.layout) {
                return self.refresh_resident_bc(last, size);
            }
            // Read through the copy, which stands unmapped pages: memory of
            // big pools is too slow to ask page by page.
            with_read(size, |read| {
                crate::host::copy_mapped(read.as_mut_ptr(), self.memory, size, false);
                self.detile(read);
            });
            #[cfg(feature = "diagnostics")]
            crate::copystat::add(crate::copystat::C::TexWatched, size as u64);
            return size;
        }
        // View memory has no write-watch, and a texture there was hashed
        // whole at every check: 229 MB and 51 ms a frame in a gameplay view
        // where two textures change (run 58). It is guarded now, as buffers
        // in view memory are: a write of the title's to it faults once and
        // is noted, a mapping is announced by the runtime. While neither
        // happened the texture is as it was. What the guard cannot see (a
        // write to the same pages through another view) is still found by
        // the hash, which is taken every eighth frame only, by the
        // texture's address, so the textures take turns.
        // `COMET_TEXTURE_GUARD=0` hashes at every check, as before.
        static GUARD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *GUARD.get_or_init(|| std::env::var_os("COMET_TEXTURE_GUARD").is_none_or(|v| v != "0"))
            && let Some((now, written)) = waited(1, || crate::watch::writes(start, start + size, true))
        {
            const HASH_EVERY: u64 = 8;
            let unwritten = last.asked != 0 && written.iter().all(|&(_, w)| w <= last.asked);
            last.asked = now;
            let turn = crate::frame::current().wrapping_add((start >> 16) as u64).is_multiple_of(HASH_EVERY);
            if unwritten && !turn {
                return 0;
            }
        }
        if resident_bc_layout(&self.layout) {
            return self.refresh_resident_bc(last, size);
        }
        // From here on the texture's memory may be read in place. Held until
        // that read is done: the runtime changes what title memory shows
        // only with this taken alone, so what `committed` finds mapped
        // stays mapped (see `crate::watch::title_memory_reading`; without
        // it a page went between the asking and the read in runs 71 and
        // 74).
        let mapped = if no_wait() {
            // Not waited for: while the runtime remaps title memory the
            // texture is read through the kernel instead, whole or not at
            // all (`no_wait`).
            let Some(mapped) = crate::watch::title_memory_reading_range_now(self.memory as usize, size) else {
                return self.refresh_unlocked(last, size);
            };
            mapped
        } else {
            waited(2, || crate::watch::title_memory_reading_range(self.memory as usize, size))
        };
        if !committed(self.memory, size) {
            // Partly mapped: a streamed texture with only some mips' pages
            // there, the usual state while it streams in. Read through the
            // copy, which stands unmapped pages (zeros), and upload what
            // is there; nothing at all mapped changes nothing.
            // shortcut: read again at most every 30 frames (reading around
            // unmapped pages is slow, and nothing tells us when pages
            // arrive): a mip that streams in shows up to 30 frames late.
            let frame = crate::frame::current();
            if last.partial.is_some_and(|at| frame < at + 30) {
                return 0;
            }
            last.partial = Some(frame);
            #[cfg(feature = "diagnostics")]
            crate::copystat::add(crate::copystat::C::TexHashed, size as u64);
            let changed = with_snapshot(size, mapped, |read| {
                crate::host::copy_mapped(read.as_mut_ptr(), self.memory, size, false);
            }, |read| {
                let hash = blake3::hash(read);
                if last.hash == Some(hash) {
                    return false;
                }
                last.hash = Some(hash);
                if read.iter().all(|&b| b == 0) {
                    return false;
                }
                self.detile(read);
                true
            });
            #[cfg(feature = "diagnostics")]
            if changed {
                crate::copystat::add(crate::copystat::C::TexChanged, size as u64);
            }
            let _ = changed;
            return size;
        }
        if through_copy() {
            return with_snapshot(size, mapped, |tiled| {
                crate::host::copy_mapped(tiled.as_mut_ptr(), self.memory, size, false);
            }, |tiled| {
                let hash = blake3::hash(tiled);
                #[cfg(feature = "diagnostics")]
                crate::copystat::add(crate::copystat::C::TexHashed, size as u64);
                if last.hash == Some(hash) {
                    return size;
                }
                last.hash = Some(hash);
                self.detile(tiled);
                #[cfg(feature = "diagnostics")]
                crate::copystat::add(crate::copystat::C::TexChanged, size as u64);
                size
            });
        }
        // SAFETY: committed memory of the texture's layout size, and it
        // stays committed while `mapped` is held.
        let tiled = unsafe { std::slice::from_raw_parts(self.memory, size) };
        let hash = blake3::hash(tiled);
        #[cfg(feature = "diagnostics")]
        crate::copystat::add(crate::copystat::C::TexHashed, size as u64);
        if last.hash == Some(hash) {
            return size;
        }
        last.hash = Some(hash);
        if short_lock() {
            with_snapshot(size, mapped, |read| {
                read.copy_from_slice(tiled);
            }, |read| {
                // Title threads can write while the mapping is held.
                // Store the hash of the bytes actually converted.
                last.hash = Some(blake3::hash(read));
                self.detile(read);
            });
        } else {
            self.detile(tiled);
            drop(mapped);
        }
        #[cfg(feature = "diagnostics")]
        crate::copystat::add(crate::copystat::C::TexChanged, size as u64);
        size
    }

    /// For BC textures, use a successful full read or a complete mip span.
    /// Missing spans retain their previous staging contents. A partial
    /// read is not a valid whole-texture hash or a successful write-watch
    /// refresh; retry next frame, even if no CPU write was observed.
    fn refresh_resident_bc(&self, last: &mut Last, size: usize) -> usize {
        let frame = crate::frame::current();
        if last.partial == Some(frame) {
            last.asked = 0;
            return 0;
        }
        let mapped = if no_wait() {
            let Some(mapped) = crate::watch::title_memory_reading_range_now(self.memory as usize, size) else {
                last.asked = 0;
                return 0;
            };
            mapped
        } else {
            waited(2, || crate::watch::title_memory_reading_range(self.memory as usize, size))
        };
        with_read(size, |read| {
            let count = self.footprints.len();
            if crate::host::read_into(self.memory, read) {
                drop(mapped);
                let hash = blake3::hash(read);
                if last.hash != Some(hash) {
                    self.detile(read);
                    last.hash = Some(hash);
                    #[cfg(feature = "diagnostics")]
                    crate::copystat::add(crate::copystat::C::TexChanged, size as u64);
                }
                if last.resident_count.is_some_and(|n| n < count) {
                    tracing::info!(memory = ?self.memory, dxgi = self.footprints[0].Footprint.Format.0,
                        swizzle = self.layout.desc.swizzle.0, resident_mips = count, missing_mips = 0,
                        "R3 BC residency recovered");
                }
                last.resident_count = Some(count);
                last.resident_hashes.clear();
                last.partial = None;
                #[cfg(feature = "diagnostics")]
                crate::copystat::add(crate::copystat::C::TexHashed, size as u64);
                return size;
            }

            let hashes = read_resident_mips(&self.layout, self.memory, read, count, self.mips, self.skip);
            drop(mapped);
            last.hash = None;
            last.asked = 0;
            last.partial = Some(frame);
            last.resident_hashes.resize(count, None);
            let mut resident = 0;
            let mut updated = 0;
            for (i, hash) in hashes.into_iter().enumerate() {
                let Some(hash) = hash else { continue };
                resident += 1;
                if last.resident_hashes[i] != Some(hash) && self.detile_one(read, i) {
                    last.resident_hashes[i] = Some(hash);
                    updated += 1;
                }
            }
            if updated != 0 {
                self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if last.resident_count != Some(resident) {
                tracing::info!(memory = ?self.memory, dxgi = self.footprints[0].Footprint.Format.0,
                    swizzle = self.layout.desc.swizzle.0, resident_mips = resident,
                    missing_mips = count - resident, updated_mips = updated,
                    "R3 BC residency preserved missing mip staging");
            }
            last.resident_count = Some(resident);
            // The failed full read plus span probes cost work even though
            // some bytes were unavailable; retain the original size estimate.
            size
        })
    }

    /// A refresh while the runtime is remapping title memory ([`no_wait`]):
    /// the texture is read through the kernel, which fails if a page of it
    /// is not there at this moment. Then nothing is taken and the texture
    /// is looked at again at its next refresh (what was learnt about writes
    /// to it is forgotten, so that it is not passed over as unwritten).
    /// Returns the bytes read.
    fn refresh_unlocked(&self, last: &mut Last, size: usize) -> usize {
        use std::sync::atomic::Ordering::Relaxed;
        let read = with_read(size, |read| {
            if !crate::host::read_into(self.memory, read) {
                return false;
            }
            let hash = blake3::hash(read);
            if last.hash != Some(hash) {
                last.hash = Some(hash);
                self.detile(read);
            }
            true
        });
        if read {
            UNLOCKED[0].fetch_add(1, Relaxed);
            size
        } else {
            UNLOCKED[1].fetch_add(1, Relaxed);
            last.asked = 0;
            0
        }
    }

    /// Detiles `tiled` (the texture's memory) into the staging buffer.
    fn detile(&self, tiled: &[u8]) {
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "diagnostics")]
        if tiled.iter().all(|&b| b == 0) {
            tracing::debug!(
                memory = ?self.memory,
                desc = ?self.layout.desc,
                "a sampled texture's memory is all zero (not streamed yet, or a failed BCPack read)"
            );
        }
        for i in 0..self.footprints.len() {
            if !self.detile_one(tiled, i) {
                return;
            }
        }
    }

    fn detile_one(&self, tiled: &[u8], i: usize) -> bool {
        let footprint = &self.footprints[i];
        let rows = self.rows[i] as usize;
        let (mip, slice) = (i as u32 % self.mips + self.skip, i as u32 / self.mips);
        let f = footprint.Footprint;
        let pitch = f.RowPitch as usize;
        // GetCopyableFootprints may omit the last row's trailing padding.
        // Never construct a Rust slice beyond the actual buffer allocation.
        let Some(remaining) = self.bytes.checked_sub(footprint.Offset) else {
            return false;
        };
        let len = (pitch * rows * f.Depth as usize).min(remaining as usize);
        // SAFETY: the slice lies inside the mapped staging allocation.
        let linear = unsafe {
            std::slice::from_raw_parts_mut(self.data.add(footprint.Offset as usize), len)
        };
        if let Err(e) = self.layout.detile(tiled, mip, slice, linear, pitch, pitch * rows) {
            tracing::warn!(?e, mip, slice, "texture not detiled");
            return false;
        }
        cloth_probe::inspect(self, mip, slice, linear, pitch, f.Format.0);
        true
    }
}

/// Lets go of a title texture's staging buffer (lists that recorded an
/// upload from it hold it until they have run); the next upload makes one
/// anew.
pub(crate) fn unstage(resource: This) {
    use std::sync::atomic::Ordering::Relaxed;
    let mut map = STAGING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = map.as_mut() {
        // A partial BC upload preserves unreadable mips only while its
        // staging buffer survives. Do not immediately discard it. The
        // cap is on total live staging, not an extra unbounded cache.
        if w3_keep_partial()
            && STAGED[1].load(Relaxed) <= 512 * (1 << 20)
            && let Some(s) = map.get(&(resource as usize))
            && s.w3_background.load(Relaxed) == W3_PARTIAL
        {
            s.w3_background.store(W3_RETAINED, Relaxed);
            static RETAINED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = RETAINED.fetch_add(1, Relaxed) + 1;
            if count.is_power_of_two() {
                tracing::info!(count, memory = ?s.memory, bytes = s.bytes, "W3 retained partial BC staging");
            }
            return;
        }
        map.remove(&(resource as usize));
    }
}

/// Whether a title texture has a staging buffer: a list recorded its
/// upload, and the lists that sample it check it at submission.
pub(crate) fn is_staged(resource: This) -> bool {
    let map = STAGING.lock().unwrap_or_else(|e| e.into_inner());
    map.as_ref().and_then(|map| map.get(&(resource as usize))).is_some_and(|s| {
        !w3_keep_partial()
            || s.w3_background.load(std::sync::atomic::Ordering::Relaxed) != W3_RETAINED
    })
}

/// The bytes of a title texture's memory, and, when requested, whether
/// all of it is mapped. A caller already retrying a partial placement
/// needs only the size; the boolean is true when the check is omitted.
pub(crate) fn staged(
    resource: This,
    check_whole: bool,
    probe: Option<PlacedResidency>,
) -> Option<(u64, bool)> {
    let s = {
        let map = STAGING.lock().unwrap_or_else(|e| e.into_inner());
        map.as_ref()?.get(&(resource as usize))?.clone()
    };
    let size = s.read_len;
    let whole = !check_whole
        || probe.is_some_and(|p| p.covers_whole(s.memory, size))
        || committed(s.memory, size);
    if w3_keep_partial() && resident_bc_layout(&s.layout) && (!check_whole || !whole) {
        s.w3_background.store(W3_PARTIAL, std::sync::atomic::Ordering::Relaxed);
    }
    Some((size as u64, whole))
}

/// Whether `[at, at + len)` is committed memory (streamed textures may be
/// unmapped until their pages arrive).
pub(crate) fn committed(at: *const u8, len: usize) -> bool {
    if by_query() {
        return committed_by_query(at, len);
    }
    if len == 0 {
        return true;
    }
    let (there, of) = readable(at as usize, len, true);
    there == of
}

/// Whether the two questions above and below are put to `VirtualQuery`, as
/// they were (`COMET_COMMITTED_QUERY=1`). `VirtualQuery` answers with the
/// length of the run of pages in the same state, which it finds by walking
/// the page tables to the run's end: 8.8 ms for one question in the title's
/// 1.1 GB block (round 6). The pass over the heap's views asks three or
/// more of them for every texture slot that is due, 3,000 slots a frame
/// when the card has no room (run 75): that pass then ran 208-264 ms, and
/// while it ran the submitting thread's write-watch askings waited behind
/// the walks (run 76: 8-22 askings a frame of 6-21 ms each, 46-172 ms).
fn by_query() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_COMMITTED_QUERY").is_some_and(|v| v == "1"))
}

/// Of the kernel's pages that hold `[at, at + len)` (`len > 0`): how many
/// can be read, and how many there are; with `all`, it stops at the first
/// that cannot. One question to the kernel for up to 256 pages at once
/// (`QueryWorkingSetEx`: the state of each page asked about, no walk); a
/// page not in the working set may still be there (never touched, or
/// trimmed), and a one-byte read through the kernel says.
/// A page that is committed and may not be read counts as not there, which
/// is what the callers ask for: they read or write the memory next.
fn readable(at: usize, len: usize, all: bool) -> (usize, usize) {
    use windows_sys::Win32::System::ProcessStatus::PSAPI_WORKING_SET_EX_INFORMATION;
    const KERNEL_PAGE: usize = 4096;
    let (first, last) = (at / KERNEL_PAGE, at.saturating_add(len - 1) / KERNEL_PAGE);
    let of = last - first + 1;
    let mut there = 0;
    // SAFETY: plain data.
    let mut info: [PSAPI_WORKING_SET_EX_INFORMATION; 256] = unsafe { std::mem::zeroed() };
    let mut page = first;
    while page <= last {
        let n = (last - page + 1).min(info.len());
        for (i, entry) in info[..n].iter_mut().enumerate() {
            entry.VirtualAddress = ((page + i) * KERNEL_PAGE) as *mut _;
        }
        let asked = query_working_set(&mut info[..n]);
        for entry in &info[..n] {
            if sample_readable(entry, asked) {
                there += 1;
            } else if all {
                return (there, of);
            }
        }
        page += n;
    }
    (there, of)
}

/// Ask about a batch of arbitrary pages (they need not be contiguous).
fn query_working_set(
    entries: &mut [windows_sys::Win32::System::ProcessStatus::PSAPI_WORKING_SET_EX_INFORMATION],
) -> bool {
    use windows_sys::Win32::System::ProcessStatus::K32QueryWorkingSetEx;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: entries is writable for its byte length; our own process.
    unsafe {
        K32QueryWorkingSetEx(
            GetCurrentProcess(),
            entries.as_mut_ptr().cast(),
            size_of_val(entries) as u32,
        ) != 0
    }
}

/// A page absent from the working set might still be committed (trimmed
/// or not yet touched). Preserve the one-byte kernel read fallback, also
/// when the whole working-set query fails.
fn sample_readable(
    entry: &windows_sys::Win32::System::ProcessStatus::PSAPI_WORKING_SET_EX_INFORMATION,
    asked: bool,
) -> bool {
    // SAFETY: the flags are plain data filled in by the query.
    let valid = asked && unsafe { entry.VirtualAttributes.Flags } & 1 != 0;
    valid || crate::host::read_into(entry.VirtualAddress.cast::<u8>(), &mut [0u8])
}

/// Estimate the mapped bytes by sampling the first page intersecting
/// each 64 KiB console block, exactly as the per-block loop did. Batch
/// the samples into one working-set query per 256 blocks rather than
/// one query per block. The first/last partial blocks contribute only
/// their bytes in the requested range.
fn mapped_blocks(start: usize, end: usize) -> u64 {
    use windows_sys::Win32::System::ProcessStatus::PSAPI_WORKING_SET_EX_INFORMATION;
    const BLOCK: usize = 64 << 10;
    const PAGE: usize = 4096;
    // SAFETY: plain data, populated before each query.
    let mut info: [PSAPI_WORKING_SET_EX_INFORMATION; 256] = unsafe { std::mem::zeroed() };
    let mut block = start & !(BLOCK - 1);
    let mut there = 0u64;
    while block < end {
        let mut n = 0;
        while n < info.len() && block < end {
            let lo = block.max(start);
            info[n].VirtualAddress = (lo & !(PAGE - 1)) as *mut _;
            block = block.saturating_add(BLOCK);
            n += 1;
        }
        let asked = query_working_set(&mut info[..n]);
        for entry in &info[..n] {
            if sample_readable(entry, asked) {
                let block = entry.VirtualAddress as usize & !(BLOCK - 1);
                let lo = block.max(start);
                let hi = block.saturating_add(BLOCK).min(end);
                there += (hi - lo) as u64;
            }
        }
    }
    there
}

fn committed_by_query(at: *const u8, len: usize) -> bool {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, VirtualQuery};
    let mut p = at as usize;
    let end = p + len;
    while p < end {
        // SAFETY: plain data filled in by the query.
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a valid out pointer of the right size.
        let n = unsafe { VirtualQuery(p as *const _, &mut info, size_of_val(&info)) };
        if n == 0 || info.State != MEM_COMMIT {
            return false;
        }
        p = info.BaseAddress as usize + info.RegionSize;
    }
    true
}

/// For a small placed texture, batch the endpoint gate and the block
/// samples in one working-set call. A missing endpoint still uses the
/// kernel read fallback: a trimmed page isn't necessarily unmapped.
fn mapped_small(start: usize, end: usize) -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::PSAPI_WORKING_SET_EX_INFORMATION;
    const BLOCK: usize = 64 << 10;
    const PAGE: usize = 4096;
    const MAX_BLOCKS: usize = 16;
    let first = start & !(BLOCK - 1);
    let last = (end - 1) & !(BLOCK - 1);
    let blocks = (last - first) / BLOCK + 1;
    if blocks > MAX_BLOCKS {
        return None;
    }
    // SAFETY: plain query structures, initialized before the syscall.
    let mut entries: [PSAPI_WORKING_SET_EX_INFORMATION; MAX_BLOCKS + 1] =
        unsafe { std::mem::zeroed() };
    for (i, entry) in entries[..blocks].iter_mut().enumerate() {
        let lo = (first + i * BLOCK).max(start);
        entry.VirtualAddress = (lo & !(PAGE - 1)) as *mut _;
    }
    // The first endpoint is always sample 0; reuse the last sample too
    // if it happens to cover the last endpoint's page.
    let tail_page = (end - 1) & !(PAGE - 1);
    let tail = if entries[blocks - 1].VirtualAddress as usize == tail_page {
        blocks - 1
    } else {
        entries[blocks].VirtualAddress = tail_page as *mut _;
        blocks
    };
    let count = blocks.max(tail + 1);
    let asked = query_working_set(&mut entries[..count]);
    let first_readable = sample_readable(&entries[0], asked);
    // A single block's only sample is its first page. If absent, even a
    // readable last endpoint cannot make the sampled byte count nonzero.
    if blocks == 1 {
        return Some(if first_readable { (end - start) as u64 } else { 0 });
    }
    let tail_readable = if first_readable { None } else { Some(sample_readable(&entries[tail], asked)) };
    if tail_readable == Some(false) {
        return Some(0);
    }
    let mut there = 0;
    for (i, entry) in entries[..blocks].iter().enumerate() {
        let readable = if i == 0 {
            first_readable
        } else if i == tail {
            tail_readable.unwrap_or_else(|| sample_readable(entry, asked))
        } else {
            sample_readable(entry, asked)
        };
        if readable {
            let block = first + i * BLOCK;
            there += (block.saturating_add(BLOCK).min(end) - block.max(start)) as u64;
        }
    }
    Some(there)
}

/// An advisory residency observation for the background scheduler.
#[derive(Clone, Copy)]
pub(crate) struct PlacedResidency {
    start: usize,
    len: usize,
    pub(crate) mapped: u64,
    whole: bool,
}

impl PlacedResidency {
    /// Only reuse an exact observation of the same staging range. It
    /// never grants permission to dereference memory; refresh still
    /// takes its own mapping lock and checks residency before reading.
    fn covers_whole(self, at: *const u8, len: usize) -> bool {
        self.whole && self.start == at as usize && self.len == len
    }
}

/// For up to sixteen kernel pages, gather the block samples and the
/// entire working-set observation in one syscall. Only claim `whole`
/// when every page was valid in that query; an invalid non-sample
/// page might merely be trimmed, so leave exact checking to the
/// existing path. Missing samples retain their one-byte fallback.
fn probe_small_placed(start: usize, end: usize) -> Option<(u64, bool)> {
    use windows_sys::Win32::System::ProcessStatus::PSAPI_WORKING_SET_EX_INFORMATION;
    const PAGE: usize = 4096;
    const BLOCK: usize = 64 << 10;
    const MAX_PAGES: usize = 16;
    if start >= end {
        return None;
    }
    let first = start / PAGE;
    let last = (end - 1) / PAGE;
    let pages = last - first + 1;
    if pages > MAX_PAGES {
        return None;
    }
    // SAFETY: plain query structures, initialized before the syscall.
    let mut entries: [PSAPI_WORKING_SET_EX_INFORMATION; MAX_PAGES] =
        unsafe { std::mem::zeroed() };
    for (i, entry) in entries[..pages].iter_mut().enumerate() {
        entry.VirtualAddress = ((first + i) * PAGE) as *mut _;
    }
    let entries = &mut entries[..pages];
    let asked = query_working_set(entries);
    // SAFETY: the query flags are plain data. As in sample_readable,
    // a valid bit suffices for this advisory residency observation.
    let whole = asked && entries.iter().all(|entry| unsafe {
        entry.VirtualAttributes.Flags & 1 != 0
    });
    if whole {
        return Some(((end - start) as u64, true));
    }
    let first_readable = sample_readable(&entries[0], asked);
    let first_block = start & !(BLOCK - 1);
    let last_block = (end - 1) & !(BLOCK - 1);
    if first_block == last_block {
        return Some((if first_readable { (end - start) as u64 } else { 0 }, false));
    }
    // A small range can span at most two console blocks. Keep the
    // existing endpoint gate and account for partial first/last blocks.
    let tail_readable = if first_readable {
        None
    } else {
        Some(sample_readable(&entries[pages - 1], asked))
    };
    if tail_readable == Some(false) {
        return Some((0, false));
    }
    let split = last_block / PAGE - first;
    let second_readable = if split == pages - 1 {
        tail_readable.unwrap_or_else(|| sample_readable(&entries[split], asked))
    } else {
        sample_readable(&entries[split], asked)
    };
    let mapped = if first_readable { last_block - start } else { 0 }
        + if second_readable { end - last_block } else { 0 };
    Some((mapped as u64, false))
}

pub(crate) fn placed_residency(at: *const u8, len: usize) -> PlacedResidency {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let on = *ON.get_or_init(|| {
        let on = std::env::var_os("COMET_HEAP_TEXTURE_PROBE").is_some_and(|v| v == "1");
        tracing::info!(on, "combined small placed texture residency probe");
        on
    });
    let start = at as usize;
    let end = start.saturating_add(len);
    let combined = if on && !by_query() && !mapped_by_page() && end - start == len {
        probe_small_placed(start, end)
    } else {
        None
    };
    let (mapped, whole) = combined.unwrap_or_else(|| (mapped_bytes(at, len), false));
    PlacedResidency { start, len, mapped, whole }
}

fn mapped_by_page() -> bool {
    static BY_PAGE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *BY_PAGE.get_or_init(|| std::env::var_os("COMET_MAPPED_BY_PAGE").is_some_and(|v| v == "1"))
}

/// How many bytes of `[at, at + len)` are committed memory: all of a
/// texture that is there, some of one streaming in (its smaller mips lie
/// at the start of its memory, and their pages are mapped first), none of
/// one not there. With neither the first nor the last page committed the
/// answer is none without a look at the pages between (two queries for the
/// slots whose texture is away, which are asked again and again).
pub(crate) fn mapped_bytes(at: *const u8, len: usize) -> u64 {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, VirtualQuery};
    if len == 0 {
        return 0;
    }
    let start = at as usize;
    let end = start.saturating_add(len);
    if start >= end {
        return 0;
    }
    let by_page = mapped_by_page();
    let query = by_query();
    if !query && !by_page
        && let Some(bytes) = mapped_small(start, end)
    {
        return bytes;
    }
    if !committed(at, 1) && !committed((end - 1) as *const u8, 1) {
        return 0;
    }
    if !query {
        if by_page {
            // By page, without the walk: whole pages, cut to the range.
            let (there, _) = readable(start, len, false);
            return (there as u64 * 4096).min(len as u64);
        }
        // By 64 KiB block, one page asked of each: the title's memory is
        // mapped in blocks of that size (the console's page), and the
        // answer is a measure of how much of a texture has arrived, not a
        // promise that every byte can be read (the upload reads through the
        // kernel). By page, a texture half there cost a failed kernel read
        // for each of its missing pages, every frame it stayed due: 1,663
        // textures "uploaded in part" kept the pass over the heap's views at
        // 90-160 ms a frame (run 81). `COMET_MAPPED_BY_PAGE=1`: by page.
        return mapped_blocks(start, end);
    }
    let (mut p, mut there) = (start, 0u64);
    while p < end {
        // SAFETY: plain data filled in by the query.
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a valid out pointer of the right size.
        let n = unsafe { VirtualQuery(p as *const _, &mut info, size_of_val(&info)) };
        let next = (info.BaseAddress as usize).saturating_add(info.RegionSize).min(end);
        if n == 0 || next <= p {
            break;
        }
        if info.State == MEM_COMMIT {
            there += (next - p) as u64;
        }
        p = next;
    }
    there
}

/// With the `skip` largest levels of a one-slice 2D texture left out: the
/// bytes from the texture's start that hold every level kept, if they all
/// lie before the largest level left out (the console's layout puts mip 0
/// last). `None`: read it all.
fn kept_prefix(layout: &comet_formats::texture::Layout, skip: u32) -> Option<usize> {
    if skip == 0 || layout.slices != 1 {
        return None;
    }
    let first_left_out = layout.subresource_read_span(skip - 1, 0)?.start;
    let kept_end = (skip..layout.desc.mips)
        .map(|m| layout.subresource_read_span(m, 0).map(|r| r.end))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max()?;
    (first_left_out > 0 && kept_end <= first_left_out).then_some(first_left_out)
}

/// Bytes of title memory not read because their levels are left out.
pub(crate) static SKIPPED_READ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The staging state of a title texture, made on first use.
fn staging(resource: This, texture: &ID3D12Resource) -> Option<Arc<Staging>> {
    let mut map = STAGING.lock().unwrap_or_else(|e| e.into_inner());
    let map = map.get_or_insert_with(HashMap::new);
    if let Some(s) = map.get(&(resource as usize)) {
        return Some(s.clone());
    }
    let (memory, desc) = crate::resource::texture_memory(resource)?;
    let Some(layout) = crate::resource::texture_layout(&desc).map(|t| t.planes[0].layout.clone())
    else {
        once!(
            ?desc,
            "texture contents without a console layout not uploaded"
        );
        return None;
    };
    if layout.desc.swizzle.xor() {
        once!(?desc, "texture contents in an XOR mode not uploaded");
        return None;
    }
    let host = host()?;
    // SAFETY: a live resource.
    let d = unsafe { texture.GetDesc() };
    let count = if d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        u32::from(d.MipLevels)
    } else {
        u32::from(d.MipLevels) * u32::from(d.DepthOrArraySize)
    };
    let mut footprints = vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); count as usize];
    let mut rows = vec![0u32; count as usize];
    let mut total = 0;
    // SAFETY: outputs for `count` subresources.
    unsafe {
        host.device.GetCopyableFootprints(
            &d,
            0,
            count,
            0,
            Some(footprints.as_mut_ptr()),
            Some(rows.as_mut_ptr()),
            None,
            Some(&mut total),
        );
    }
    let (buffer, data) = host.upload_buffer(total)?;
    if resident_bc_layout(&layout) {
        // A recorded copy can precede the first successful refresh.
        // Initialize missing mips once; later partial reads preserve them.
        // SAFETY: upload_buffer mapped total writable bytes.
        unsafe { data.write_bytes(0, total as usize) };
    }
    crate::host::name(&buffer, || {
        format!(
            "staging of {resource:?} {}x{}x{} f{}",
            d.Width, d.Height, d.DepthOrArraySize, d.Format.0
        )
    });
    tracing::debug!(?desc, total, "texture contents uploaded from title memory");
    if w3_full_snapshot(&layout) {
        // A recorded copy may run before the first readable snapshot.
        // SAFETY: upload_buffer mapped total writable bytes.
        unsafe { data.write_bytes(0, total as usize) };
    }
    STAGED[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    STAGED[1].fetch_add(total, std::sync::atomic::Ordering::Relaxed);
    let skip = if d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D && d.DepthOrArraySize == 1 {
        layout.desc.mips.saturating_sub(u32::from(d.MipLevels))
    } else {
        0
    };
    let read_len = kept_prefix(&layout, skip).unwrap_or(layout.size as usize);
    if skip > 0 {
        SKIPPED_READ.fetch_add(layout.size.saturating_sub(read_len as u64), std::sync::atomic::Ordering::Relaxed);
    }
    let s = Arc::new(Staging {
        buffer,
        #[cfg(feature = "diagnostics")]
        size: total,
        data,
        footprints,
        rows,
        layout,
        mips: u32::from(d.MipLevels),
        skip,
        read_len,
        bytes: total,
        memory,
        last: Mutex::default(),
        dirty: std::sync::atomic::AtomicBool::new(true),
        uploaded: std::sync::atomic::AtomicBool::new(false),
        used: std::sync::atomic::AtomicU64::new(crate::host::moment()),
        w3_background: std::sync::atomic::AtomicU8::new(0),
    });
    map.insert(resource as usize, s.clone());
    Some(s)
}

#[cfg(test)]
mod title_memory_tests {
    use super::{with_snapshot, with_title};

    #[test]
    fn w3_complete_snapshot_rejects_partial_copy_and_accepts_zero() {
        let mut scratch = [0xabu8; 32];
        let previous = super::w3_complete_hash(&mut scratch, |read| {
            read.fill(0x6d);
            true
        }).unwrap();
        assert_eq!(previous, blake3::hash(&[0x6d; 32]));
        assert_eq!(super::w3_complete_hash(&mut scratch, |read| {
            read[..8].fill(0x7e);
            false
        }), None);
        assert_eq!(&scratch[..8], &[0x7e; 8]);
        let zero = super::w3_complete_hash(&mut scratch, |read| {
            read.fill(0);
            true
        }).unwrap();
        assert_eq!(zero, blake3::hash(&[0; 32]));
        assert_ne!(zero, previous);
    }

    #[test]
    fn w3_complete_snapshot_retries_after_real_page_remap() {
        use windows_sys::Win32::System::Memory::*;
        const PAGE: usize = 4096;
        // SAFETY: private reservation; only committed pages touched directly.
        unsafe {
            let base = VirtualAlloc(std::ptr::null(), 2 * PAGE, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE).cast::<u8>();
            assert!(!base.is_null());
            base.write_bytes(0x6d, 2 * PAGE);
            let mut scratch = vec![0xab; 2 * PAGE];
            let original = super::w3_complete_hash(&mut scratch, |dst| crate::host::read_into(base, dst));
            assert_eq!(original, Some(blake3::hash(&vec![0x6d; 2 * PAGE])));
            assert_ne!(VirtualFree(base.add(PAGE).cast(), PAGE, MEM_DECOMMIT), 0);
            assert_eq!(super::w3_complete_hash(&mut scratch, |dst| crate::host::read_into(base, dst)), None);
            assert_eq!(VirtualAlloc(base.add(PAGE).cast(), PAGE, MEM_COMMIT, PAGE_READWRITE), base.add(PAGE).cast());
            base.add(PAGE).write_bytes(0x7e, PAGE);
            let recovered = super::w3_complete_hash(&mut scratch, |dst| crate::host::read_into(base, dst));
            let mut expected = vec![0x6d; 2 * PAGE];
            expected[PAGE..].fill(0x7e);
            assert_eq!(recovered, Some(blake3::hash(&expected)));
            assert_ne!(recovered, original);
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    #[test]
    fn private_snapshot_releases_mapping_before_conversion() {
        use std::cell::Cell;
        struct Lock<'a>(&'a Cell<bool>);
        impl Drop for Lock<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let held = Cell::new(true);
        let source = [11, 22, 33, 44];
        let hash = with_snapshot(source.len(), Lock(&held), |read| {
            assert!(held.get());
            read.copy_from_slice(&source);
        }, |read| {
            assert!(!held.get(), "conversion must not hold the mapping lock");
            assert_eq!(read, source);
            blake3::hash(read)
        });
        assert_eq!(hash, blake3::hash(&source));
    }

    #[test]
    fn batched_block_samples_match_single_queries_after_remaps() {
        use windows_sys::Win32::System::Memory::*;
        const BLOCK: usize = 64 << 10;
        const SIZE: usize = 260 * BLOCK;
        // SAFETY: private reservation; only committed pages are accessed.
        unsafe {
            let base = VirtualAlloc(std::ptr::null(), SIZE, MEM_RESERVE, PAGE_NOACCESS).cast::<u8>();
            assert!(!base.is_null());
            let start = base as usize + 4096 + 37;
            let end = base as usize + SIZE - 123;
            let commit = |offset: usize| {
                let page = base.add(offset);
                assert_eq!(VirtualAlloc(page.cast(), 4096, MEM_COMMIT, PAGE_READWRITE), page.cast());
                page.write(0x5a);
            };
            // First (partial), interior, both sides of the 256-entry
            // batch boundary, and last (partial) block. A hole remains
            // between all other samples.
            for offset in [4096, 10 * BLOCK, 255 * BLOCK, 256 * BLOCK, 259 * BLOCK] {
                commit(offset);
            }
            let serial = || {
                let mut sum = 0;
                for block in (base as usize..end).step_by(BLOCK) {
                    let lo = block.max(start);
                    let hi = (block + BLOCK).min(end);
                    if super::readable(lo, 1, true).0 == 1 {
                        sum += (hi - lo) as u64;
                    }
                }
                sum
            };
            let expected = (5 * BLOCK - 4096 - 37 - 123) as u64;
            assert_eq!(serial(), expected);
            assert_eq!(super::mapped_blocks(start, end), expected);
            assert_ne!(VirtualFree(base.add(255 * BLOCK).cast(), 4096, MEM_DECOMMIT), 0);
            commit(258 * BLOCK);
            assert_eq!(super::mapped_blocks(start, end), serial());
            assert_ne!(VirtualFree(base.add(259 * BLOCK).cast(), 4096, MEM_DECOMMIT), 0);
            assert_eq!(super::mapped_blocks(start, end), expected - (BLOCK - 123) as u64);
            assert_eq!(super::mapped_blocks(start, end), serial());
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    #[test]
    fn small_texture_samples_match_endpoint_gate_and_serial_blocks() {
        use windows_sys::Win32::System::Memory::*;
        const BLOCK: usize = 64 << 10;
        const PAGE: usize = 4096;
        // SAFETY: private reservation, only committed pages are touched.
        unsafe {
            let base = VirtualAlloc(std::ptr::null(), 17 * BLOCK, MEM_RESERVE, PAGE_NOACCESS).cast::<u8>();
            assert!(!base.is_null());
            // Independent first, middle, last-block and last-endpoint
            // pages, including a last block whose first page is absent.
            let offsets = [PAGE, BLOCK, 2 * BLOCK, 2 * BLOCK + 2 * PAGE];
            let start = base as usize + PAGE + 37;
            let end = base as usize + offsets[3] + 19;
            for mask in 0..16 {
                for (i, offset) in offsets.into_iter().enumerate() {
                    let page = base.add(offset);
                    if mask & (1 << i) != 0 {
                        assert_eq!(VirtualAlloc(page.cast(), PAGE, MEM_COMMIT, PAGE_READWRITE), page.cast());
                        page.write(1);
                    } else {
                        assert_ne!(VirtualFree(page.cast(), PAGE, MEM_DECOMMIT), 0);
                    }
                }
                for (a, b) in [
                    (start, end),
                    (start, base as usize + BLOCK - 13),
                    (start, base as usize + 2 * BLOCK + 19),
                    (base as usize + BLOCK, base as usize + 2 * BLOCK),
                ] {
                    let endpoints = super::readable(a, 1, true).0 != 0
                        || super::readable(b - 1, 1, true).0 != 0;
                    let expected = if endpoints { super::mapped_blocks(a, b) } else { 0 };
                    assert_eq!(super::mapped_small(a, b), Some(expected), "mask={mask} range={a:#x}..{b:#x}");
                }
            }
            assert_eq!(super::mapped_small(base as usize, base as usize + 17 * BLOCK), None);
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    #[test]
    fn small_combined_probe_preserves_samples_and_exact_range_identity() {
        use windows_sys::Win32::System::Memory::*;
        const PAGE: usize = 4096;
        const BLOCK: usize = 64 << 10;
        // SAFETY: private reservation; only committed pages are touched.
        unsafe {
            let base = VirtualAlloc(std::ptr::null(), 2 * BLOCK, MEM_RESERVE, PAGE_NOACCESS).cast::<u8>();
            assert!(!base.is_null());
            let start = base as usize + BLOCK - 2 * PAGE + 13;
            let end = base as usize + BLOCK + 2 * PAGE - 19;
            let offsets = [BLOCK - 2 * PAGE, BLOCK - PAGE, BLOCK, BLOCK + PAGE];
            for mask in 0..16 {
                for (i, offset) in offsets.into_iter().enumerate() {
                    let page = base.add(offset);
                    if mask & (1 << i) != 0 {
                        assert_eq!(VirtualAlloc(page.cast(), PAGE, MEM_COMMIT, PAGE_READWRITE), page.cast());
                        page.write(1);
                    } else {
                        assert_ne!(VirtualFree(page.cast(), PAGE, MEM_DECOMMIT), 0);
                    }
                }
                for (a, b) in [(start, end), (start, base as usize + BLOCK - 7)] {
                    let (mapped, whole) = super::probe_small_placed(a, b).unwrap();
                    assert_eq!(Some(mapped), super::mapped_small(a, b));
                    if whole {
                        let (there, pages) = super::readable(a, b - a, true); assert_eq!(there, pages);
                    }
                    let observation = super::PlacedResidency { start: a, len: b - a, mapped, whole };
                    assert_eq!(observation.covers_whole(a as *const u8, b - a), whole);
                    assert!(!observation.covers_whole((a + 1) as *const u8, b - a));
                    assert!(!observation.covers_whole(a as *const u8, b - a - 1));
                }
            }
            assert!(super::probe_small_placed(base as usize, base as usize + 2 * BLOCK).is_none());
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    /// Title memory whose tail is reserved and not committed reads through
    /// the copy as its bytes and then zeros, without a fault; so does
    /// memory a page of which is taken away between two reads, and the
    /// zeros are zeros whatever the read buffer held before.
    #[test]
    fn an_uncommitted_tail_reads_as_zeros() {
        use windows_sys::Win32::System::Memory::*;
        const HEAD: usize = 0xb_0000;
        const SIZE: usize = 0x20_0000;
        // SAFETY: a private reservation of our own; only committed pages
        // are written in place, and nothing else reads them in place.
        unsafe {
            // The read buffer holds something else first.
            let other = vec![0x77u8; SIZE];
            with_title(other.as_ptr(), SIZE, |bytes| assert!(bytes.iter().all(|&b| b == 0x77)));
            let base = VirtualAlloc(std::ptr::null(), SIZE, MEM_RESERVE, PAGE_NOACCESS).cast::<u8>();
            assert!(!base.is_null());
            assert_eq!(VirtualAlloc(base.cast(), HEAD, MEM_COMMIT, PAGE_READWRITE), base.cast());
            base.write_bytes(0x5a, HEAD);
            with_title(base, SIZE, |bytes| {
                assert!(bytes[..HEAD].iter().all(|&b| b == 0x5a), "what is mapped");
                assert!(bytes[HEAD..].iter().all(|&b| b == 0), "what is not: zeros");
            });
            // The title takes a page in the middle away: zeros from then
            // on, the rest as before.
            assert_ne!(VirtualFree(base.add(0x4_0000).cast(), 0x1_0000, MEM_DECOMMIT), 0);
            with_title(base, SIZE, |bytes| {
                assert!(bytes[..0x4_0000].iter().all(|&b| b == 0x5a));
                assert!(bytes[0x4_0000..0x5_0000].iter().all(|&b| b == 0));
                assert!(bytes[0x5_0000..HEAD].iter().all(|&b| b == 0x5a));
                assert!(bytes[HEAD..].iter().all(|&b| b == 0));
            });
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    #[test]
    fn resident_bc_mips_reject_holes_and_accept_zero_data() {
        use comet_formats::texture::{Dimension, Swizzle, Texture, TextureDesc};
        use windows_sys::Win32::System::Memory::*;
        let texture = Texture::new(&TextureDesc {
            dimension: Dimension::Tex2d, width: 256, height: 256,
            depth_or_array: 1, mips: 0, format: 98, samples: 1,
            swizzle: Swizzle::SW_64KB_D,
        }).unwrap();
        let l = &texture.planes[0].layout;
        let size = l.size as usize;
        assert_eq!(size, 131072);
        // SAFETY: private reservation, only committed pages accessed directly.
        unsafe {
            let base = VirtualAlloc(std::ptr::null(), size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE).cast::<u8>();
            assert!(!base.is_null());
            base.write_bytes(0, 65536);
            base.add(65536).write_bytes(0x6d, 65536);
            let mut scratch = vec![0xab; size];
            let before = super::read_resident_mips(l, base, &mut scratch, 9, l.desc.mips, 0);
            assert!(before.iter().all(Option::is_some));
            let mut top = vec![0; 65536];
            l.detile(&scratch, 0, 0, &mut top, 1024, 65536).unwrap();
            assert!(top.iter().all(|&b| b == 0x6d));
            assert_ne!(VirtualFree(base.add(65536 + 4096).cast(), 4096, MEM_DECOMMIT), 0);
            let partial = super::read_resident_mips(l, base, &mut scratch, 9, l.desc.mips, 0);
            assert!(partial[0].is_none());
            assert_eq!(partial[1..], before[1..]); // genuine zeros are resident
            if partial[0].is_some() {
                l.detile(&scratch, 0, 0, &mut top, 1024, 65536).unwrap();
            }
            assert!(top.iter().all(|&b| b == 0x6d));
            let page = base.add(65536 + 4096);
            assert_eq!(VirtualAlloc(page.cast(), 4096, MEM_COMMIT, PAGE_READWRITE), page.cast());
            page.write_bytes(0x7e, 4096);
            let recovered = super::read_resident_mips(l, base, &mut scratch, 9, l.desc.mips, 0);
            assert!(recovered[0].is_some());
            assert_ne!(recovered[0], before[0]);
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    /// The texture both device-loss runs named last (16 texels wide, 4 or 8
    /// high, format 10, layout 0x100, console flags 0x300000; its memory
    /// began 0x300 into a page in run 73 and 0x700 in run 72, and here also
    /// 0xf00, so that a page line runs through it): what the detiling is
    /// given and what it writes are the same read in place and read through
    /// the copy, and the rows it writes end where the staging buffer the
    /// host sizes for it ends (896 bytes for 4 rows of 128 bytes 256 apart,
    /// 1,920 for 8).
    #[test]
    fn the_small_texture_of_runs_72_and_73_detiles_the_same_both_ways() {
        use windows_sys::Win32::System::Memory::*;
        for (height, within) in [(4u32, 0x300usize), (8, 0x700), (4, 0xf00), (8, 0xf00)] {
            // The description as the log printed it, word for word.
            let words: [u32; 14] = [3, 0, 0, 0, 16, 0, height, 65537, 10, 1, 0, 256, 3_145_728, 0];
            // SAFETY: fourteen words are a resource description (56 bytes,
            // plain numbers).
            let desc: crate::resource::ResourceDesc = unsafe { std::mem::transmute(words) };
            let layout = crate::resource::texture_layout(&desc)
                .expect("a console layout")
                .planes[0]
                .layout
                .clone();
            let size = layout.size as usize;
            let (row, pitch) = (16 * 8, 256usize);
            assert!(size >= row * height as usize, "the layout holds every texel");
            // Committed pages and a reserved one after them; the texture
            // begins `within` bytes into the first, as the title placed it.
            // SAFETY: our own reservation; the committed part is written
            // and read in place, by this thread alone.
            unsafe {
                let pages = size.div_ceil(4096) + 1;
                let base = VirtualAlloc(std::ptr::null(), (pages + 1) * 4096, MEM_RESERVE, PAGE_NOACCESS).cast::<u8>();
                assert!(!base.is_null());
                assert_eq!(VirtualAlloc(base.cast(), pages * 4096, MEM_COMMIT, PAGE_READWRITE), base.cast());
                for i in 0..pages * 4096 {
                    base.add(i).write((i * 7 + i / 251) as u8);
                }
                let memory = base.add(within);
                let in_place = std::slice::from_raw_parts(memory, size);
                // The bytes: the same in place and through the copy.
                with_title(memory, size, |copied| assert_eq!(copied, in_place));
                // What the detiling writes: the same, and inside the bytes
                // the host's staging buffer for this texture has.
                let staged = (height as usize - 1) * pitch + row;
                assert_eq!(staged, if height == 4 { 896 } else { 1920 });
                let detile = |tiled: &[u8]| {
                    // The slice `Staging::detile` hands over: whole rows.
                    let mut linear = vec![0xeeu8; pitch * height as usize];
                    layout
                        .detile(tiled, 0, 0, &mut linear, pitch, pitch * height as usize)
                        .expect("detiled");
                    linear
                };
                let direct = detile(in_place);
                let through = with_title(memory, size, |copied| detile(copied));
                assert_eq!(direct, through);
                for y in 0..height as usize {
                    assert!(direct[y * pitch..][..row].iter().any(|&b| b != 0xee), "row {y} written");
                    assert!(direct[y * pitch + row..(y + 1) * pitch].iter().all(|&b| b == 0xee), "nothing past row {y}");
                }
                // A buffer of exactly the staged bytes is enough.
                let mut exact = vec![0u8; staged];
                layout
                    .detile(in_place, 0, 0, &mut exact, pitch, pitch * height as usize)
                    .expect("the staging buffer's bytes are enough");
                assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
            }
        }
    }
}
