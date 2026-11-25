//! Texture staging buffers as slices of a few large upload buffers
//! (`COMET_STAGING_POOL`).
//!
//! Every texture the bindless pass made ready got an upload buffer of its
//! own: a committed resource made, mapped and zeroed by the driver, then
//! released when the list that copied from it was reset. On the reference
//! machine the pass spent ~230 of its 250 ms a frame in the phase that makes
//! host textures and their staging ("host view/upload recording"), and the
//! code counted 42,607 upload buffers (77.9 GB) made in five minutes of one
//! run. Here a staging buffer is a range of a 64 MiB upload buffer kept
//! mapped for good; a range comes back when its staging is dropped (after
//! the list that copies from it was reset, as the whole buffer did before).
//! A range used before is zeroed when it is handed out again, so a texture
//! with mips not yet streamed still shows zeros there, as a new buffer did.

use std::collections::BTreeMap;

/// Offsets and lengths are multiples of this (D3D12's placement alignment
/// for texture data in a buffer).
pub(crate) const ALIGN: u64 = 512;

/// A chunk's free ranges, by offset.
pub(crate) struct Ranges {
    free: BTreeMap<u64, u64>,
    size: u64,
    /// Bytes from the start ever handed out: below it a range may hold an
    /// earlier texture's bytes.
    high: u64,
}

impl Ranges {
    pub(crate) fn new(size: u64) -> Ranges {
        let size = size / ALIGN * ALIGN;
        Ranges { free: BTreeMap::from([(0, size)]), size, high: 0 }
    }

    /// The first free range of `len` bytes (rounded up to [`ALIGN`]):
    /// its offset, and whether any of it was handed out before.
    pub(crate) fn take(&mut self, len: u64) -> Option<(u64, bool)> {
        let len = len.max(1).div_ceil(ALIGN) * ALIGN;
        let (&at, &have) = self.free.iter().find(|&(_, &have)| have >= len)?;
        self.free.remove(&at);
        if have > len {
            self.free.insert(at + len, have - len);
        }
        let used = at < self.high;
        self.high = self.high.max(at + len);
        Some((at, used))
    }

    /// Gives back what [`take`](Self::take) handed out at `at` for `len`.
    pub(crate) fn give(&mut self, at: u64, len: u64) {
        let mut at = at;
        let mut len = len.max(1).div_ceil(ALIGN) * ALIGN;
        // Joined with the free range just after it, and the one just before.
        if let Some(next) = self.free.remove(&(at + len)) {
            len += next;
        }
        if let Some((&before, &have)) = self.free.range(..at).next_back()
            && before + have == at
        {
            self.free.remove(&before);
            at = before;
            len += have;
        }
        self.free.insert(at, len);
    }

    /// Whether nothing of it is handed out.
    pub(crate) fn empty(&self) -> bool {
        self.free.get(&0) == Some(&self.size)
    }
}

/// Whether staging buffers are pool slices (`COMET_STAGING_POOL=1`).
pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var_os("COMET_STAGING_POOL").is_some_and(|v| v == "1");
        tracing::info!(on, "texture staging buffers as slices of pooled upload buffers");
        on
    })
}

#[cfg(windows)]
pub(crate) use host::{STATS, release, slice};

#[cfg(windows)]
mod host {
    use super::{ALIGN, Ranges};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

    /// Bytes of one pooled upload buffer.
    const CHUNK: u64 = 64 << 20;
    /// Larger staging buffers get one of their own, as before.
    pub(crate) const MOST: u64 = CHUNK / 4;
    /// Empty chunks kept for later; more are released.
    const KEEP_EMPTY: usize = 2;

    struct Chunk {
        buffer: ID3D12Resource,
        data: *mut u8,
        ranges: Ranges,
    }

    // SAFETY: the mapping is persistent; ranges are handed out under POOL's
    // lock and each is written by its one owner.
    unsafe impl Send for Chunk {}

    static POOL: Mutex<Vec<Option<Chunk>>> = Mutex::new(Vec::new());

    /// For the memory line: slices handed out, chunks made, chunks
    /// released, slices zeroed for reuse, and microseconds spent handing out.
    pub(crate) static STATS: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];

    /// A staging buffer's range of a pooled upload buffer.
    pub(crate) struct Slice {
        pub(crate) buffer: ID3D12Resource,
        /// Its first byte in `buffer`.
        pub(crate) offset: u64,
        /// The mapping of its first byte.
        pub(crate) data: *mut u8,
        pub(crate) chunk: u32,
        pub(crate) len: u64,
    }

    /// A range of `len` bytes for a staging buffer; `None` when `len` is
    /// over [`MOST`] or no upload buffer could be made (the caller then
    /// makes one of its own).
    pub(crate) fn slice(host: &crate::host::Host, len: u64) -> Option<Slice> {
        if len > MOST {
            return None;
        }
        let started = std::time::Instant::now();
        let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
        let found = pool.iter_mut().enumerate().find_map(|(i, c)| {
            let c = c.as_mut()?;
            c.ranges.take(len).map(|(at, used)| (i, at, used))
        });
        let (index, at, used) = match found {
            Some(found) => found,
            None => {
                let (buffer, data) = host.upload_buffer(CHUNK)?;
                crate::host::name(&buffer, || "pooled texture staging".to_string());
                STATS[1].fetch_add(1, Relaxed);
                let mut ranges = Ranges::new(CHUNK);
                let (at, used) = ranges.take(len)?;
                let chunk = Chunk { buffer, data, ranges };
                let index = match pool.iter().position(Option::is_none) {
                    Some(i) => {
                        pool[i] = Some(chunk);
                        i
                    }
                    None => {
                        pool.push(Some(chunk));
                        pool.len() - 1
                    }
                };
                (index, at, used)
            }
        };
        let chunk = pool[index].as_ref()?;
        let len = len.max(1).div_ceil(ALIGN) * ALIGN;
        // SAFETY: `at + len` lies within the chunk's mapping.
        let data = unsafe { chunk.data.add(at as usize) };
        if used {
            STATS[3].fetch_add(1, Relaxed);
            // SAFETY: the range is this caller's alone from here on.
            unsafe { data.write_bytes(0, len as usize) };
        }
        STATS[0].fetch_add(1, Relaxed);
        let slice = Slice { buffer: chunk.buffer.clone(), offset: at, data, chunk: index as u32, len };
        drop(pool);
        STATS[4].fetch_add(started.elapsed().as_micros() as u64, Relaxed);
        Some(slice)
    }

    /// Gives a slice's range back (its staging is gone: no list copies
    /// from it any more).
    pub(crate) fn release(chunk: u32, offset: u64, len: u64) {
        let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
        let Some(Some(c)) = pool.get_mut(chunk as usize) else {
            return;
        };
        c.ranges.give(offset, len);
        if c.ranges.empty() && pool.iter().flatten().filter(|c| c.ranges.empty()).count() > KEEP_EMPTY {
            pool[chunk as usize] = None;
            STATS[2].fetch_add(1, Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ALIGN, Ranges};

    #[test]
    fn ranges_take_align_and_join_on_give() {
        let mut r = Ranges::new(8 * ALIGN);
        assert_eq!(r.take(1), Some((0, false)));
        assert_eq!(r.take(ALIGN + 1), Some((ALIGN, false)));
        assert_eq!(r.take(5 * ALIGN), Some((3 * ALIGN, false)));
        assert_eq!(r.take(1), None);
        r.give(ALIGN, ALIGN + 1);
        // Reused: handed out before, so the caller zeroes it.
        assert_eq!(r.take(2 * ALIGN), Some((ALIGN, true)));
        r.give(0, 1);
        r.give(ALIGN, 2 * ALIGN);
        r.give(3 * ALIGN, 5 * ALIGN);
        assert!(r.empty());
        assert_eq!(r.take(8 * ALIGN), Some((0, true)));
    }

    #[test]
    fn ranges_first_fit_skips_small_holes() {
        let mut r = Ranges::new(10 * ALIGN);
        let a = r.take(ALIGN).unwrap().0;
        let _b = r.take(ALIGN).unwrap().0;
        r.give(a, ALIGN);
        // The one-block hole at 0 is too small: the rest is used.
        assert_eq!(r.take(2 * ALIGN), Some((2 * ALIGN, false)));
        assert_eq!(r.take(ALIGN), Some((0, true)));
    }
}
