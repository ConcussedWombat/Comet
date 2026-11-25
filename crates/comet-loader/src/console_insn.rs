//! Instructions the console's CPU (Zen 2) has and an Intel PC's lacks, emulated: the title's
//! code raises STATUS_ILLEGAL_INSTRUCTION on them (the title zeroes memory with a `clzero` loop at
//! its first frames), and the vectored handler steps over them here.
//!
//! Ported from XCL (github.com/ivall/xcl, GPL-2.0-only): `insn.rs` (decoder, as
//! `console_insn_decode.rs`, unchanged) and `eh.rs` `emulate_console_only`.

#[path = "console_insn_decode.rs"]
mod decode;

use decode::ConsoleOnly;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use windows_sys::Win32::System::Diagnostics::Debug::CONTEXT;

const CONTINUE_EXECUTION: i32 = -1;

/// Whether what Windows said about memory is remembered per thread (the regions it found
/// writable, the code bytes it found readable) and not asked again at every emulated
/// instruction. A query walks the pages of the region it answers for: asked from outside, it
/// took 2-9 ms in the title's heaps of 0.6-2.2 GB and 89 ms in its 6.2 GB page pool (run 30),
/// and the title zeroes blocks of them with `clzero` loops all through a frame: six of its
/// threads spent 25-78 % of their time inside that query (runs 29 and 30), about 27 s of
/// processor time in Windows per 9 s frame. On by default; `COMET_CLZERO_ASK=1` asks every
/// time, as before.
fn remembers() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_CLZERO_ASK").is_none_or(|v| v != "1"))
}

/// Answers a remembered region gives before Windows is asked about it again: one freed or
/// protected meanwhile is forgotten then.
const ANSWERS: u32 = 4096;

thread_local! {
    /// The regions Windows last said are committed and writable (start, end), newest first,
    /// and the answers they gave since they were emptied.
    static WRITABLE: Cell<([(usize, usize); 4], u32)> = const { Cell::new(([(0, 0); 4], 0)) };
    static FAST_WRITABLE: WritableCache = const { WritableCache::new() };
    /// Faulting addresses and the code bytes found readable there, newest first.
    static CODE: Cell<[(usize, usize); 8]> = const { Cell::new([(0, 0); 8]) };
}

/// Instructions emulated, the times Windows was asked about memory for them, and the bytes
/// `clzero` loops zeroed, since the start.
static EMULATED: AtomicU64 = AtomicU64::new(0);
static ASKED: AtomicU64 = AtomicU64::new(0);
static ZEROED: AtomicU64 = AtomicU64::new(0);

fn readable(va: usize, bytes: usize) -> bool {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_NOACCESS, VirtualQuery};
    // SAFETY: plain data filled in by the query.
    let mut m: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a valid out pointer of the right size.
    let found = unsafe { VirtualQuery(va as *const _, &mut m, size_of_val(&m)) } != 0;
    ASKED.fetch_add(1, Relaxed);
    found && m.State == MEM_COMMIT && m.Protect & (PAGE_NOACCESS | PAGE_GUARD) == 0 && m.BaseAddress as usize + m.RegionSize >= va + bytes
}

/// Same four regions, order and expiration policy as WRITABLE. Only the
/// answer counter changes on an ordinary hit; avoid copying the full TLS
/// tuple back and avoid a second TLS lookup. No new permissions are cached.
struct WritableCache {
    regions: Cell<[(usize, usize); 4]>,
    answers: Cell<u32>,
}

impl WritableCache {
    const fn new() -> Self {
        Self { regions: Cell::new([(0, 0); 4]), answers: Cell::new(0) }
    }

    fn hit(&self, va: usize, end: usize) -> Option<usize> {
        let regions = self.regions.get();
        let &(_, to) = regions.iter().find(|&&(from, to)| from <= va && va < to)?;
        let answers = self.answers.get();
        if answers >= ANSWERS {
            self.regions.set([(0, 0); 4]);
            self.answers.set(0);
        } else {
            self.answers.set(answers + 1);
        }
        Some(to.min(end))
    }

    fn insert(&self, from: usize, to: usize) {
        let mut regions = self.regions.get();
        regions.rotate_right(1);
        regions[0] = (from, to);
        self.regions.set(regions);
    }
}

fn fast_writable_cache() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_P4_CLZERO_CACHE_HIT").is_some_and(|v| v == "1"))
}

fn writable(va: usize, bytes: usize) -> bool {
    writable_to(va, va + bytes) == Some(va + bytes)
}

/// Where the committed, writable memory that `va` lies in ends, at most `end`; None if `va`
/// is not in such memory. A region found is remembered ([`remembers`]).
/// shortcut: a remembered region that was freed or protected since is written all the same
/// for up to [`ANSWERS`] answers: the write then faults inside the handler, which the guard
/// on view memory opens as it does any write, and which is a crash otherwise (as the
/// unchecked single-line emulation behind this one already is).
fn writable_to(va: usize, end: usize) -> Option<usize> {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualQuery};
    let remember = remembers();
    let fast = remember && fast_writable_cache();
    if fast {
        if let Some(to) = FAST_WRITABLE.with(|cache| cache.hit(va, end)) {
            return Some(to);
        }
    } else if remember {
        let (regions, answers) = WRITABLE.with(Cell::get);
        if let Some(&(_, to)) = regions.iter().find(|&&(from, to)| from <= va && va < to) {
            let next = if answers >= ANSWERS { ([(0, 0); 4], 0) } else { (regions, answers + 1) };
            WRITABLE.with(|w| w.set(next));
            return Some(to.min(end));
        }
    }
    // SAFETY: plain data filled in by the query.
    let mut m: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a valid out pointer of the right size.
    let found = unsafe { VirtualQuery(va as *const _, &mut m, size_of_val(&m)) } != 0;
    ASKED.fetch_add(1, Relaxed);
    if !found || m.State != MEM_COMMIT || m.Protect & (PAGE_READWRITE | PAGE_EXECUTE_READWRITE) == 0 {
        return None;
    }
    let (from, to) = (m.BaseAddress as usize, m.BaseAddress as usize + m.RegionSize);
    if fast {
        FAST_WRITABLE.with(|cache| cache.insert(from, to));
    } else if remember {
        WRITABLE.with(|w| {
            let (mut regions, answers) = w.get();
            regions.rotate_right(1);
            regions[0] = (from, to);
            w.set((regions, answers));
        });
    }
    Some(to.min(end))
}

/// The warning has process-wide once-per-instruction-kind semantics. Avoid taking a
/// global mutex from every vectored exception after the kind has been seen.
fn lock_free_seen() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_P4_INSN_SEEN").is_some_and(|v| v == "1"))
}

fn instruction_kind(what: ConsoleOnly) -> usize {
    match what {
        ConsoleOnly::Ud2 => 0,
        ConsoleOnly::Clwb => 1,
        ConsoleOnly::Monitorx => 2,
        ConsoleOnly::Mwaitx => 3,
        ConsoleOnly::Clzero => 4,
        ConsoleOnly::ClzeroLoop { .. } => 5,
        ConsoleOnly::Rdpru => 6,
        ConsoleOnly::Rdpid(_) => 7,
        ConsoleOnly::Extrq { .. } => 8,
        ConsoleOnly::Insertq { .. } => 9,
    }
}

fn first_kind(seen: &AtomicU64, what: ConsoleOnly) -> bool {
    let bit = 1u64 << instruction_kind(what);
    seen.load(Relaxed) & bit == 0 && seen.fetch_or(bit, Relaxed) & bit == 0
}

fn warn_once(what: ConsoleOnly, rip: usize) {
    if lock_free_seen() {
        static SEEN: AtomicU64 = AtomicU64::new(0);
        if first_kind(&SEEN, what) && what != ConsoleOnly::Ud2 {
            tracing::warn!(instruction = ?what, at = format_args!("{rip:#x}"), "a console CPU instruction this CPU lacks: emulated (logged once)");
        }
    } else {
        static SEEN: std::sync::Mutex<Vec<std::mem::Discriminant<ConsoleOnly>>> = std::sync::Mutex::new(Vec::new());
        if let Ok(mut seen) = SEEN.lock()
            && !seen.contains(&std::mem::discriminant(&what))
        {
            seen.push(std::mem::discriminant(&what));
            if what != ConsoleOnly::Ud2 {
                tracing::warn!(instruction = ?what, at = format_args!("{rip:#x}"), "a console CPU instruction this CPU lacks: emulated (logged once)");
            }
        }
    }
}

/// An illegal instruction at `ctx.Rip`: a console-only one is emulated and stepped over
/// (`CONTINUE_EXECUTION`); anything else returns 0 (the crash report and the title's handlers
/// see it).
///
/// # Safety
/// `ctx` is the faulting thread's context, from the exception being dispatched.
pub(crate) unsafe fn emulate(ctx: &mut CONTEXT) -> i32 {
    let rip = ctx.Rip as usize;
    // The same few instructions fault again and again: their bytes are asked about once.
    let known = if remembers() { CODE.with(Cell::get) } else { [(0, 0); 8] };
    let n = match known.iter().find(|&&(at, n)| at == rip && n > 0) {
        Some(&(_, n)) => n,
        None => {
            if !readable(rip, 1) {
                return 0;
            }
            let n = (1..=16).rev().find(|&n| readable(rip, n)).unwrap_or(0);
            if remembers() {
                let mut known = known;
                known.rotate_right(1);
                known[0] = (rip, n);
                CODE.with(|c| c.set(known));
            }
            n
        }
    };
    // SAFETY: `n` bytes at `rip` are readable.
    let code = unsafe { std::slice::from_raw_parts(rip as *const u8, n) };
    let Some((what, len)) = decode::decode(code) else { return 0 };
    warn_once(what, rip);
    match what {
        ConsoleOnly::Ud2 => return 0,
        ConsoleOnly::Clwb => {
            // nothing to write back on the PC: patched to NOPs, no exception next time
            use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
            let mut old = 0u32;
            // SAFETY: the instruction's own bytes, made writable for the patch and restored.
            unsafe {
                if VirtualProtect(rip as *const _, len, PAGE_EXECUTE_READWRITE, &mut old) != 0 {
                    std::slice::from_raw_parts_mut(rip as *mut u8, len).fill(0x90);
                    VirtualProtect(rip as *const _, len, old, &mut old);
                }
            }
        }
        ConsoleOnly::Monitorx => {}
        ConsoleOnly::Mwaitx => std::hint::spin_loop(),
        ConsoleOnly::Clzero => {
            // with an address-size prefix (0x67) the line is at EAX, zero-extended
            let a32 = code[..len - 3].contains(&0x67);
            let line = (if a32 { ctx.Rax & 0xFFFF_FFFF } else { ctx.Rax } as usize) & !63;
            if !writable(line, 64) {
                return 0; // the real instruction faults there too
            }
            // SAFETY: the 64-byte line is committed and writable.
            unsafe { std::ptr::write_bytes(line as *mut u8, 0, 64) };
        }
        ConsoleOnly::ClzeroLoop { end_reg } => {
            // the whole `clzero; add rax,40h; cmp rax,end; jb` loop at once
            let regs: [u64; 16] = [
                ctx.Rax, ctx.Rcx, ctx.Rdx, ctx.Rbx, ctx.Rsp, ctx.Rbp, ctx.Rsi, ctx.Rdi, ctx.R8, ctx.R9, ctx.R10, ctx.R11, ctx.R12, ctx.R13, ctx.R14, ctx.R15,
            ];
            let end = regs[end_reg as usize & 15] as usize;
            let start = ctx.Rax as usize;
            if end > start && end - start < 2 << 30 {
                // Region by region: a block may lie across several (the title's pages are
                // mapped one view each), and one answer for all of it was a refusal, after
                // which every 64-byte line took an exception of its own.
                let mut at = start;
                while at < end {
                    let Some(to) = writable_to(at, end) else { break };
                    // SAFETY: committed and writable from `at` to `to`.
                    unsafe { std::ptr::write_bytes(at as *mut u8, 0, to - at) };
                    at = to;
                }
                if at == start {
                    return 0;
                }
                ZEROED.fetch_add((at - start) as u64, Relaxed);
                if at < end {
                    // What follows is not writable: the loop goes on there (the same
                    // instruction again), where the real one faults too.
                    ctx.Rax = at as u64;
                    return CONTINUE_EXECUTION;
                }
            }
            ctx.Rax = end as u64;
        }
        ConsoleOnly::Rdpru => {
            let mut v = 0i64;
            // SAFETY: a valid out pointer.
            unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut v) };
            let v = if ctx.Rcx as u32 <= 1 { v as u64 } else { 0 };
            ctx.Rax = v & 0xFFFF_FFFF;
            ctx.Rdx = v >> 32;
        }
        ConsoleOnly::Rdpid(r) => {
            // SAFETY: no preconditions.
            let cpu = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessorNumber() } as u64;
            let regs: [&mut u64; 16] = [
                &mut ctx.Rax, &mut ctx.Rcx, &mut ctx.Rdx, &mut ctx.Rbx, &mut ctx.Rsp, &mut ctx.Rbp, &mut ctx.Rsi, &mut ctx.Rdi, &mut ctx.R8, &mut ctx.R9, &mut ctx.R10, &mut ctx.R11, &mut ctx.R12, &mut ctx.R13,
                &mut ctx.R14, &mut ctx.R15,
            ];
            *regs[r as usize & 15] = cpu;
        }
        // the low quadword changes; the high one is zeroed, as a Zen 5 does
        ConsoleOnly::Extrq { dst, src, len, idx } => {
            // SAFETY: the context's XMM registers (CONTEXT_FLOATING_POINT is captured for exceptions).
            let x = unsafe { &mut ctx.Anonymous.FltSave.XmmRegisters };
            let (len, idx) = match src {
                Some(s) => {
                    let c = x[s as usize & 15].Low;
                    (c as u8, (c >> 8) as u8)
                }
                None => (len, idx),
            };
            let d = &mut x[dst as usize & 15];
            d.Low = decode::extrq(d.Low, len, idx);
            d.High = 0;
        }
        ConsoleOnly::Insertq { dst, src, reg, len, idx } => {
            // SAFETY: as above.
            let x = unsafe { &mut ctx.Anonymous.FltSave.XmmRegisters };
            let s = x[src as usize & 15];
            let (len, idx) = if reg { (s.High as u8, (s.High >> 8) as u8) } else { (len, idx) };
            let d = &mut x[dst as usize & 15];
            d.Low = decode::insertq(d.Low, s.Low, len, idx);
            d.High = 0;
        }
    }
    ctx.Rip += len as u64;
    // How often this runs at all (each one is an exception: a trip into Windows and back).
    let emulated = EMULATED.fetch_add(1, Relaxed) + 1;
    if emulated >= 1 << 16 && emulated.is_power_of_two() {
        tracing::info!(
            emulated,
            asked = ASKED.load(Relaxed),
            zeroed_mb = ZEROED.load(Relaxed) >> 20,
            p4_seen_mutex_per_instruction = !lock_free_seen(),
            "console CPU instructions emulated so far, the times Windows was asked about memory for them, and what clzero loops zeroed"
        );
    }
    CONTINUE_EXECUTION
}

#[cfg(test)]
mod p4_tests {
    use super::*;

    #[test]
    fn p4_seen_one_warning_per_kind_not_per_operand() {
        let seen = AtomicU64::new(0);
        for instruction in [
            ConsoleOnly::Ud2, ConsoleOnly::Clwb, ConsoleOnly::Monitorx,
            ConsoleOnly::Mwaitx, ConsoleOnly::Clzero,
            ConsoleOnly::ClzeroLoop { end_reg: 2 }, ConsoleOnly::Rdpru,
            ConsoleOnly::Rdpid(0),
            ConsoleOnly::Extrq { dst: 0, src: None, len: 1, idx: 0 },
            ConsoleOnly::Insertq { dst: 0, src: 1, reg: false, len: 1, idx: 0 },
        ] {
            assert!(first_kind(&seen, instruction));
            assert!(!first_kind(&seen, instruction));
        }
        assert_eq!(seen.load(Relaxed), (1 << 10) - 1);
        assert!(!first_kind(&seen, ConsoleOnly::Rdpid(9)));
        assert!(!first_kind(&seen, ConsoleOnly::ClzeroLoop { end_reg: 10 }));
    }

    #[test]
    fn p4_fast_writable_matches_original_cache() {
        let fast = WritableCache::new();
        let (mut regions, mut answers) = ([(0usize, 0usize); 4], 0u32);
        let mut state = 0x1234_5678u64;
        for step in 0..(ANSWERS * 5) {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let va = 0x1000 + ((state >> 32) as usize % 5) * 0x1000 + 64;
            let end = va + ((state >> 24) as usize & 0x1fff);
            let old = if let Some(&(_, to)) = regions.iter().find(|&&(from, to)| from <= va && va < to) {
                if answers >= ANSWERS {
                    regions = [(0, 0); 4];
                    answers = 0;
                } else {
                    answers += 1;
                }
                Some(to.min(end))
            } else {
                None
            };
            assert_eq!(fast.hit(va, end), old, "step {step}");
            if old.is_none() && step % 7 != 0 {
                let from = va & !0xfff;
                let to = from + 0x1000;
                regions.rotate_right(1);
                regions[0] = (from, to);
                fast.insert(from, to);
            }
            assert_eq!(fast.regions.get(), regions, "step {step}");
            assert_eq!(fast.answers.get(), answers, "step {step}");
        }
    }

    #[test]
    fn p4_seen_concurrent_first_kind_has_one_winner() {
        let seen = AtomicU64::new(0);
        let winners = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8).map(|_| {
                scope.spawn(|| first_kind(&seen, ConsoleOnly::Clzero))
            }).collect();
            workers.into_iter().map(|worker| worker.join().unwrap() as usize).sum::<usize>()
        });
        assert_eq!(winners, 1);
    }
}
