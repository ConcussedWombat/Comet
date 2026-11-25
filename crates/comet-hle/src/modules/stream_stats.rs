//! Wall-clock streaming telemetry; no scheduling or completion changes.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

pub(super) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_STREAM_STATS").is_some_and(|v| v == "1"))
}
#[derive(Debug)]
pub(super) struct Key<'a> {
    pub path: &'a str,
    pub offset: u64,
    pub source: u32,
    pub dest: u32,
    pub options: u64,
    pub intermediate: u32,
}
type OwnedKey = (String, u64, u32, u32, u64, u32);
impl Key<'_> {
    fn owned(&self) -> OwnedKey {
        (self.path.to_owned(), self.offset, self.source, self.dest, self.options, self.intermediate)
    }
}
pub(super) struct Ticket {
    at: Instant,
    retry_of: Option<u64>,
}
#[derive(Default)]
pub(super) struct Depth {
    pub unsubmitted: usize,
    pub waiting: usize,
    pub running: usize,
    pub statuses: usize,
    pub staged: u64,
    pub staging_limit: u64,
}
// 1 ms bins; final bin means >= 10 s.
const MAX_MS: usize = 10_000;
const MAX_FAILED: usize = 4096;
#[derive(Default)]
struct Window {
    enqueued: u64, submitted: u64, completed: u64, failed: u64, canceled: u64,
    written: u64, physical_written: u64, physical_failed: u64, source_bytes: u64,
    file_calls: u64, file_bytes: u64, file_failed: u64, async_calls: u64,
    crt_calls: u64, crt_bytes: u64, crt_short: u64,
    polls: u64, ready_polls: u64, published: u64, published_failed: u64,
    repeat_submits: u64, repeat_ok: u64, tracking_overflow: u64,
    histogram: Vec<u64>, busy_ns: u128, peak_busy: usize,
}
impl Window {
    fn new() -> Self { Self { histogram: vec![0; MAX_MS + 1], ..Self::default() } }
}
struct Stats {
    since: Instant, changed: Instant, busy: usize, window: Window,
    failed_keys: HashMap<OwnedKey, u64>,
}
static STATS: LazyLock<Mutex<Stats>> = LazyLock::new(|| {
    let now = Instant::now();
    Mutex::new(Stats { since: now, changed: now, busy: 0, window: Window::new(), failed_keys: HashMap::new() })
});
fn stats() -> MutexGuard<'static, Stats> {
    STATS.lock().unwrap_or_else(|e| e.into_inner())
}
pub(super) fn init() { drop(stats()); }
std::thread_local! { static SEQ: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) }; }
pub(super) fn set_sequence(seq: Option<u64>) { if enabled() { SEQ.set(seq); } }
pub(super) fn sequence() -> Option<u64> { SEQ.get() }
fn update(f: impl FnOnce(&mut Window)) {
    if enabled() { f(&mut stats().window); }
}
impl Stats {
    fn integrate(&mut self, now: Instant) {
        self.window.busy_ns += now.saturating_duration_since(self.changed).as_nanos() * self.busy as u128;
        self.changed = now;
    }
}
pub(super) fn enqueued() { update(|w| w.enqueued += 1); }
pub(super) fn submitted(key: Option<Key<'_>>, seq: u64) -> Option<Ticket> {
    if !enabled() { return None; }
    let mut s = stats();
    s.window.submitted += 1;
    let retry_of = if s.failed_keys.is_empty() { None } else {
        key.as_ref().and_then(|k| s.failed_keys.get(&k.owned()).copied())
    };
    if let Some(prior) = retry_of {
        s.window.repeat_submits += 1;
        tracing::info!(seq, prior, key = ?key, "N8 stream same file range resubmitted");
    }
    Some(Ticket { at: Instant::now(), retry_of })
}
pub(super) fn started() {
    if !enabled() { return; }
    let mut s = stats();
    s.integrate(Instant::now());
    s.busy += 1;
    let busy = s.busy;
    s.window.peak_busy = s.window.peak_busy.max(busy);
}
pub(super) fn finished(ticket: Option<&Ticket>, key: Option<Key<'_>>, seq: u64,
    result: Result<(), i32>) {
    if !enabled() { return; }
    let Some(ticket) = ticket else { return; };
    let mut s = stats();
    s.integrate(Instant::now());
    s.busy = s.busy.saturating_sub(1);
    s.window.completed += 1;
    let bin = ticket.at.elapsed().as_millis().min(MAX_MS as u128) as usize;
    s.window.histogram[bin] += 1;
    match result {
        Ok(()) => {
            if let (Some(prior), Some(k)) = (ticket.retry_of, key) {
                let owned = k.owned();
                if s.failed_keys.get(&owned) == Some(&prior) { s.failed_keys.remove(&owned); }
                s.window.repeat_ok += 1;
                tracing::info!(seq, prior, key = ?k, "N8 stream repeated file range succeeded");
            }
        }
        Err(hr) => {
            s.window.failed += 1;
            if let Some(k) = key {
                let owned = k.owned();
                if s.failed_keys.len() < MAX_FAILED || s.failed_keys.contains_key(&owned) {
                    s.failed_keys.insert(owned, seq);
                } else { s.window.tracking_overflow += 1; }
                tracing::warn!(seq, hr = format_args!("{:#010x}", hr as u32), key = ?k,
                    "N8 stream file request failed");
            } else {
                tracing::warn!(seq, hr = format_args!("{:#010x}", hr as u32),
                    "N8 stream request failed (no stable file identity)");
            }
        }
    }
}
pub(super) fn canceled(n: usize) { update(|w| w.canceled += n as u64); }
pub(super) fn source_bytes(n: usize) { update(|w| w.source_bytes += n as u64); }
pub(super) fn written(n: usize, physical: bool) {
    update(|w| { w.written += n as u64; if physical { w.physical_written += n as u64; } });
}
pub(super) fn physical_failed() { update(|w| w.physical_failed += 1); }
pub(super) fn file_read(bytes: usize, failed: bool, asynchronous: bool) {
    update(|w| { w.file_calls += 1; w.file_bytes += bytes as u64;
        w.file_failed += u64::from(failed); w.async_calls += u64::from(asynchronous); });
}
pub(super) fn crt_read(bytes: usize, short: bool) {
    update(|w| { w.crt_calls += 1; w.crt_bytes += bytes as u64; w.crt_short += u64::from(short); });
}
pub(super) fn poll(ready: bool) {
    update(|w| { w.polls += 1; w.ready_polls += u64::from(ready); });
}
pub(super) fn published(failed: bool) {
    update(|w| { w.published += 1; w.published_failed += u64::from(failed); });
}
fn percentile(histogram: &[u64], percent: u64) -> Option<usize> {
    let total: u64 = histogram.iter().sum();
    if total == 0 { return None; }
    let rank = (total * percent).div_ceil(100);
    let mut cumulative = 0;
    histogram.iter().position(|n| { cumulative += n; cumulative >= rank })
}
pub(super) fn report(workers: usize, depth: Depth) {
    let (w, seconds, busy, outstanding) = {
        let mut s = stats();
        let now = Instant::now();
        s.integrate(now);
        let seconds = now.saturating_duration_since(s.since).as_secs_f64().max(0.001);
        s.since = now;
        let w = std::mem::replace(&mut s.window, Window::new());
        let busy = s.busy;
        s.window.peak_busy = busy;
        (w, seconds, busy, s.failed_keys.len())
    };
    let rate = |n: u64| n as f64 / seconds;
    tracing::info!(seconds, workers, busy_workers = busy, peak_busy = w.peak_busy,
        avg_busy_workers = w.busy_ns as f64 / 1e9 / seconds,
        enqueued_s = rate(w.enqueued), submitted_s = rate(w.submitted),
        completed_s = rate(w.completed), successful_s = rate(w.completed - w.failed),
        failed_s = rate(w.failed), canceled_s = rate(w.canceled),
        source_file_bytes_s = rate(w.source_bytes), written_bytes_s = rate(w.written),
        physical_written_bytes_s = rate(w.physical_written), physical_failures = w.physical_failed,
        unsubmitted = depth.unsubmitted, waiting = depth.waiting, running = depth.running,
        pending_statuses = depth.statuses, staged_bytes = depth.staged, staging_limit = depth.staging_limit,
        p50_ms = ?percentile(&w.histogram, 50), p95_ms = ?percentile(&w.histogram, 95),
        latency_over_10s = w.histogram[MAX_MS], "N8 stream stats");
    tracing::info!(seconds, status_polls_s = rate(w.polls),
        ready_polls_s = rate(w.ready_polls), statuses_published_s = rate(w.published),
        failed_statuses = w.published_failed, repeated_submits = w.repeat_submits,
        repeated_success = w.repeat_ok, outstanding_failed_ranges = outstanding,
        tracking_overflow = w.tracking_overflow, "N8 stream completion and retry stats");
    tracing::info!(seconds, readfile_calls_s = rate(w.file_calls), readfile_bytes_s = rate(w.file_bytes),
        readfile_sync_failed_s = rate(w.file_failed), readfile_async_calls_s = rate(w.async_calls),
        fread_calls_s = rate(w.crt_calls), fread_bytes_s = rate(w.crt_bytes),
        fread_short_s = rate(w.crt_short), "N8 stream file stats");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latency_histogram_nearest_rank() {
        assert_eq!(percentile(&[0, 0, 0], 95), None);
        assert_eq!(percentile(&[0, 50, 44, 6], 50), Some(1));
        assert_eq!(percentile(&[0, 50, 44, 6], 95), Some(3));
    }
}
