//! Host pipelines are made by workers, not by the title thread that first
//! records a draw with them. Translating a shader and the host driver's
//! compile take from milliseconds to seconds each (structurisation of the
//! biggest compute programs ~0.4 s, some driver compiles 3.4 s), and
//! the title's first world frame wants over 1,200 new pipelines: made
//! on its recording threads, that frame took more than 30 s and the
//! title's GPU watchdog ended the run (decision 0007, "Pipelines made off
//! the title's threads").
//!
//! A first use queues the pipeline and waits for it within a budget per
//! frame, so the cheap ones (menus, one-off passes such as a colour-grading
//! bake) are there at once as before; past the budget a draw or dispatch
//! whose pipeline is not ready is dropped, as an untranslated one is.
//!
//! The default budget is half a second per title frame. A pass the title
//! runs only once and whose pipeline takes longer is lost.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use super::Shader;
use super::host::{GraphicsState, HostPipeline};

/// What a host pipeline is made from, and the host pipeline once made.
pub(crate) struct Pending {
    shaders: Vec<Arc<Shader>>,
    pub(crate) graphics: Option<Box<GraphicsState>>,
    queue_state: AtomicU8,
    made: OnceLock<Option<HostPipeline>>,
    /// What identifies it across runs (shader containers and state), and
    /// whether a draw or dispatch has requested it yet.
    key: String,
    used: AtomicBool,
    /// R4 diagnostics: first request returned before compilation finished.
    r4_first_missed: AtomicU64,
}

// SAFETY: D3D12 objects are free-threaded; the graphics state (whose input
// elements point into its own names) is only read after creation.
unsafe impl Send for Pending {}
// SAFETY: as above.
unsafe impl Sync for Pending {}

impl Pending {
    #[cfg(test)]
    pub(crate) fn new(shaders: Vec<Arc<Shader>>, graphics: Option<Box<GraphicsState>>) -> Pending {
        let state = graphics.as_ref().map(|g| g.describe());
        Self::with_description(shaders, graphics, state.as_deref())
    }

    fn with_description(
        shaders: Vec<Arc<Shader>>,
        graphics: Option<Box<GraphicsState>>,
        state: Option<&str>,
    ) -> Pending {
        let mut key = blake3::Hasher::new();
        for s in &shaders {
            key.update(s.stage.as_bytes());
            key.update(s.hash.as_bytes());
        }
        key.update(state.unwrap_or_default().as_bytes());
        Pending {
            shaders,
            graphics,
            queue_state: AtomicU8::new(NOT_QUEUED),
            made: OnceLock::new(),
            key: key.finalize().to_hex()[..32].to_owned(),
            used: AtomicBool::new(false),
            r4_first_missed: AtomicU64::new(u64::MAX),
        }
    }

    fn make(&self) {
        self.made
            .get_or_init(|| HostPipeline::new(&self.shaders, self.graphics.as_deref()));
        let (lock, done) = &DONE;
        let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
        done.notify_all();
    }
}

/// Reuse live host pipeline work for identical title pipeline descriptions.
/// The shader objects are interned by full content hash, and the graphics
/// description includes every field used by HostPipeline. The persistent
/// early-list key remains unchanged. Weak entries do not prolong PSO life.
pub(crate) fn shared(shaders: Vec<Arc<Shader>>, graphics: Option<Box<GraphicsState>>) -> Arc<Pending> {
    static CACHE: Mutex<Option<HashMap<[u8; 32], Weak<Pending>>>> = Mutex::new(None);
    static INSERTIONS: AtomicU64 = AtomicU64::new(0);
    static REUSE: AtomicU64 = AtomicU64::new(0);
    static ENABLED: OnceLock<bool> = OnceLock::new();

    let state = graphics.as_ref().map(|g| g.describe());
    if !*ENABLED.get_or_init(|| std::env::var_os("COMET_SHARED_PIPELINES").is_none_or(|v| v != "0")) {
        return Arc::new(Pending::with_description(shaders, graphics, state.as_deref()));
    }
    let mut hash = blake3::Hasher::new();
    hash.update(&(shaders.len() as u64).to_le_bytes());
    for shader in &shaders {
        hash.update(&(Arc::as_ptr(shader) as usize).to_le_bytes());
    }
    hash.update(&[u8::from(state.is_some())]);
    if let Some(state) = &state {
        hash.update(state.as_bytes());
    }
    let key = *hash.finalize().as_bytes();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(existing) = cache.get(&key).and_then(Weak::upgrade) {
        let n = REUSE.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_power_of_two() {
            tracing::debug!(reused = n, "shared host pipeline descriptions");
        }
        return existing;
    }
    if INSERTIONS.fetch_add(1, Ordering::Relaxed).is_multiple_of(4096) {
        cache.retain(|_, p| p.strong_count() != 0);
    }
    let pending = Arc::new(Pending::with_description(shaders, graphics, state.as_deref()));
    cache.insert(key, Arc::downgrade(&pending));
    pending
}

/// Why a pipeline has no host pipeline (diagnostics): its shaders that do
/// not translate, else "refused" or "being made"; `None` if it has one.
#[cfg(feature = "diagnostics")]
pub(crate) fn missing(p: &Pending) -> Option<String> {
    match p.made.get() {
        Some(Some(_)) => None,
        None => Some("being made".into()),
        Some(None) => {
            let failed: Vec<String> = p
                .shaders
                .iter()
                .filter(|s| matches!(s.translated.get(), Some(None)))
                .map(|s| format!("{} {}", s.stage, s.hash))
                .collect();
            Some(if failed.is_empty() {
                format!("refused {:?}", p.shaders)
            } else {
                failed.join(" + ")
            })
        }
    }
}

/// Signalled whenever a worker has made a pipeline.
static DONE: (Mutex<()>, Condvar) = (Mutex::new(()), Condvar::new());

/// Waiting allowed per title frame, over all threads.
fn wait_budget() -> Duration {
    static BUDGET: OnceLock<Duration> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        let millis = std::env::var("COMET_PIPELINE_WAIT_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(500);
        tracing::info!(millis, "host pipeline wait budget per frame");
        Duration::from_millis(millis)
    })
}

/// Pipelines queued and not made yet.
static WAITING: AtomicU64 = AtomicU64::new(0);

/// Pipelines queued and not made yet (for the frame line).
pub(crate) fn waiting() -> u64 {
    WAITING.load(Ordering::Relaxed)
}

/// Explicit full log hashes only. Never prewarm or extend waits for a
/// pipeline merely because it happens to have a mesh or compute stage.
fn r4_hashes(value: &str) -> Vec<String> {
    value.split(',').map(str::trim)
        .filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase).collect()
}

fn r4_keys(value: &str) -> Vec<String> {
    value.split(',').map(str::trim)
        .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase).collect()
}

fn r4_selected(p: &Pending) -> bool {
    static HASHES: OnceLock<Vec<String>> = OnceLock::new();
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    let keys = KEYS.get_or_init(|| {
        r4_keys(&std::env::var("COMET_R4_GEOMETRY_KEYS").unwrap_or_default())
    });
    let hashes = HASHES.get_or_init(|| {
        let value = std::env::var("COMET_R4_GEOMETRY_WAIT").unwrap_or_default();
        r4_hashes(&value)
    });
    keys.contains(&p.key) || p.shaders.iter().any(|s| hashes.contains(&s.hash))
}

fn r4_priority() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R4_PREWARM_PRIORITY").is_some_and(|v| v == "1"))
}

fn r4_trace() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_R4_GEOMETRY_TRACE").is_some_and(|v| v == "1"))
}

/// A separate, bounded first-use allowance, shared across recording
/// threads. Reserved before waiting, refunded only to the same frame.
#[derive(Default)]
struct R4Grace {
    frame: Option<u64>,
    spent: Duration,
}

impl R4Grace {
    fn reserve(&mut self, frame: u64, limit: Duration) -> Duration {
        if self.frame.is_some_and(|current| frame < current) {
            return Duration::ZERO;
        }
        if self.frame != Some(frame) {
            self.frame = Some(frame);
            self.spent = Duration::ZERO;
        }
        let left = limit.saturating_sub(self.spent);
        self.spent += left;
        left
    }

    fn refund(&mut self, frame: u64, reserved: Duration, elapsed: Duration) {
        if self.frame == Some(frame) {
            self.spent = self.spent.saturating_sub(reserved.saturating_sub(elapsed));
        }
    }
}

/// A selected producer or draw may be one-shot. Avoid losing its first
/// execution merely because another PSO spent the ordinary wait budget.
fn r4_grace(p: &Pending, frame: u64) {
    static BUDGET: Mutex<R4Grace> = Mutex::new(R4Grace { frame: None, spent: Duration::ZERO });
    static LIMIT: OnceLock<Duration> = OnceLock::new();
    let limit = *LIMIT.get_or_init(|| Duration::from_millis(
        std::env::var("COMET_R4_GEOMETRY_WAIT_MS").ok()
            .and_then(|v| v.parse::<u64>().ok()).unwrap_or(250).min(500)
    ));
    let left = BUDGET.lock().unwrap_or_else(|e| e.into_inner()).reserve(frame, limit);
    if left.is_zero() {
        return;
    }
    let started = Instant::now();
    let (lock, done) = &DONE;
    let mut held = lock.lock().unwrap_or_else(|e| e.into_inner());
    while p.made.get().is_none() {
        let rest = left.saturating_sub(started.elapsed());
        if rest.is_zero() {
            break;
        }
        held = done.wait_timeout(held, rest).unwrap_or_else(|e| e.into_inner()).0;
    }
    drop(held);
    let elapsed = started.elapsed();
    BUDGET.lock().unwrap_or_else(|e| e.into_inner()).refund(frame, left, elapsed);
    #[cfg(feature = "diagnostics")]
    crate::copystat::add(crate::copystat::C::PipelineWaitUs, elapsed.as_micros() as u64);
    tracing::info!(
        key = %p.key, shaders = ?p.shaders, frame,
        waited_ms = elapsed.as_secs_f64() * 1000.0,
        finished = p.made.get().is_some(), accepted = p.made.get().is_some_and(Option::is_some),
        "R4 geometry first-use grace"
    );
}

/// The host pipeline if it is made (or is made within the frame's budget);
/// queues it on first use.
pub(crate) fn ready(p: &Arc<Pending>) -> Option<&HostPipeline> {
    // A still-pending pipeline was requested by the title too. Prewarm it
    // on the next launch, even if this run never got to use it.
    let first = !p.used.load(Ordering::Relaxed) && !p.used.swap(true, Ordering::Relaxed);
    if first {
        super::library::note_used(&p.key);
    }
    if p.made.get().is_none() {
        let frame = crate::frame::current();
        start(p, REQUESTED);
        wait(p, frame);
        if first && p.made.get().is_none() && r4_selected(p) {
            r4_grace(p, frame);
        }
    }
    // Count only a real returned miss, not a worker completing between
    // two observations of the OnceLock.
    let made = p.made.get();
    if made.is_none() && first && r4_trace() {
        let frame = crate::frame::current();
        p.r4_first_missed.store(frame, Ordering::Relaxed);
        tracing::warn!(key = %p.key, shaders = ?p.shaders, frame, "R4 geometry first use missed");
    }
    let made = made?;
    if p.r4_first_missed.load(Ordering::Relaxed) != u64::MAX {
        let missed = p.r4_first_missed.swap(u64::MAX, Ordering::Relaxed);
        if missed != u64::MAX {
            let frame = crate::frame::current();
            tracing::info!(key = %p.key, shaders = ?p.shaders, frame,
                missed_frame = missed, late_frames = frame.saturating_sub(missed),
                accepted = made.is_some(), "R4 geometry first-use miss resolved on later request");
        }
    }
    made.as_ref()
}

/// Makes `p` on this thread if it is not made yet (waiting for a worker
/// that is making it); whether it has a host pipeline.
pub(crate) fn now(p: &Arc<Pending>) -> bool {
    // The builder preflight requests every stage, even the stages after
    // a failed one. Remember them for the next launch so they can be
    // prepared during loading rather than blocking the first dispatch.
    if !p.used.load(Ordering::Relaxed) && !p.used.swap(true, Ordering::Relaxed) {
        super::library::note_used(&p.key);
    }
    p.make();
    let ready = p.made.get().is_some_and(|made| made.is_some());
    if !ready {
        let translation_failed = p
            .shaders
            .iter()
            .any(|s| matches!(s.translated.get(), Some(None)));
        tracing::warn!(
            key = %p.key,
            shaders = ?p.shaders,
            translation_failed,
            "BVH builder preflight could not create a host pipeline"
        );
    }
    ready
}

/// At the title's creation of a pipeline: starts it at once if an earlier
/// run drew with it (during loading, so the world's first frames find it).
pub(crate) fn early(p: &Arc<Pending>) {
    let selected = r4_selected(p);
    if !p.shaders.is_empty() && (super::library::wanted_early(&p.key) || selected) {
        super::library::EARLY.fetch_add(1, Ordering::Relaxed);
        start(p, if selected && r4_priority() { R4_EARLY } else { EARLY });
    }
}

/// Whether the frame's budget is one for all threads together: a thread
/// that waits takes what is left of it and gives back what it did not
/// use. Counted only when a wait had ended, the budget was one per thread:
/// each of the title's five recording threads began its first wait of a
/// frame with nothing counted yet and waited the whole half second, 2.5 s
/// of waiting in every frame in which pipelines were being made (runs
/// 34-49: "2504 ms waiting for pipelines" in each such frame).
/// On by default; `COMET_PIPELINE_WAIT_SHARED=0` counts as before.
fn shared_budget() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMET_PIPELINE_WAIT_SHARED").is_none_or(|v| v != "0"))
}

/// Waits for `p` while frame `frame` has budget left.
fn wait(p: &Pending, frame: u64) {
    let budget = wait_budget();
    if budget.is_zero() {
        return;
    }
    // The frame the budget belongs to, and what was waited of it (or is
    // taken by a thread waiting now).
    static SPENT: Mutex<(u64, Duration)> = Mutex::new((u64::MAX, Duration::ZERO));
    let shared = shared_budget();
    let left = {
        let mut spent = SPENT.lock().unwrap_or_else(|e| e.into_inner());
        if spent.0 != frame {
            *spent = (frame, Duration::ZERO);
        }
        let left = budget.saturating_sub(spent.1);
        if shared {
            spent.1 += left;
        }
        left
    };
    if left.is_zero() {
        return;
    }
    let started = Instant::now();
    let (lock, done) = &DONE;
    let mut held = lock.lock().unwrap_or_else(|e| e.into_inner());
    while p.made.get().is_none() {
        let rest = left.saturating_sub(started.elapsed());
        if rest.is_zero() {
            break;
        }
        held = done.wait_timeout(held, rest).unwrap_or_else(|e| e.into_inner()).0;
    }
    drop(held);
    let waited = started.elapsed().min(left);
    {
        let mut spent = SPENT.lock().unwrap_or_else(|e| e.into_inner());
        // A frame that began meanwhile has its own budget.
        if spent.0 == frame {
            if shared {
                spent.1 = spent.1.saturating_sub(left - waited);
            } else {
                spent.1 += waited;
            }
        }
    }
    #[cfg(feature = "diagnostics")]
    crate::copystat::add(
        crate::copystat::C::PipelineWaitUs,
        started.elapsed().as_micros() as u64,
    );
}

const NOT_QUEUED: u8 = 0;
const EARLY: u8 = 1;
const R4_EARLY: u8 = 2;
const REQUESTED: u8 = 3;
const MAKING: u8 = 4;

struct Queue {
    early: VecDeque<Arc<Pending>>,
    r4_early: VecDeque<Arc<Pending>>,
    requested: VecDeque<Arc<Pending>>,
}

impl Queue {
    const fn new() -> Self {
        Self {
            early: VecDeque::new(),
            r4_early: VecDeque::new(),
            requested: VecDeque::new(),
        }
    }

    /// Queues an unrequested pipeline or promotes one queued at creation.
    /// Returns whether it is new to the queue, or `None` if nothing changed.
    fn push(&mut self, p: &Arc<Pending>, priority: u8) -> Option<bool> {
        let prev = p.queue_state.load(Ordering::Relaxed);
        if prev >= priority {
            return None;
        }
        p.queue_state.store(priority, Ordering::Relaxed);
        match priority {
            REQUESTED => self.requested.push_back(p.clone()),
            R4_EARLY => self.r4_early.push_back(p.clone()),
            _ => self.early.push_back(p.clone()),
        }
        Some(prev == NOT_QUEUED)
    }

    fn pop(&mut self) -> Option<Arc<Pending>> {
        let p = self.requested.pop_front()
            .or_else(|| std::iter::from_fn(|| self.r4_early.pop_front())
                .find(|p| p.queue_state.load(Ordering::Relaxed) == R4_EARLY))
            .or_else(|| {
                // An early entry can have been promoted and claimed already.
                std::iter::from_fn(|| self.early.pop_front())
                    .find(|p| p.queue_state.load(Ordering::Relaxed) == EARLY)
            })?;
        p.queue_state.store(MAKING, Ordering::Relaxed);
        Some(p)
    }
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue::new());
static AVAILABLE: Condvar = Condvar::new();

/// Start or promote work for a draw without waiting behind the entire early
/// list. Queue changes and worker claims happen under the same lock.
fn start(p: &Arc<Pending>, priority: u8) {
    if p.queue_state.load(Ordering::Relaxed) >= priority {
        return;
    }
    if !workers() {
        if p.queue_state.swap(MAKING, Ordering::Relaxed) == NOT_QUEUED {
            p.make();
        }
        return;
    }
    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(new) = queue.push(p, priority) {
        if new {
            PEAK.fetch_max(WAITING.fetch_add(1, Ordering::Relaxed) + 1, Ordering::Relaxed);
        }
        if priority == R4_EARLY && r4_trace() {
            tracing::info!(key = %p.key, shaders = ?p.shaders, "R4 geometry priority prewarm");
        }
        AVAILABLE.notify_one();
    }
}

fn next() -> Arc<Pending> {
    let mut queue = QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if let Some(p) = queue.pop() {
            return p;
        }
        queue = AVAILABLE.wait(queue).unwrap_or_else(|e| e.into_inner());
    }
}

/// Starts the workers (half the logical processors, 2 to 8) on first use.
fn workers() -> bool {
    static RUNNING: OnceLock<bool> = OnceLock::new();
    *RUNNING.get_or_init(|| {
        let mut running = false;
        let workers = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(2, 8));
        for i in 0..workers {
            let spawned = std::thread::Builder::new()
                .name(format!("comet-pipelines-{i}"))
                .spawn(|| {
                    // Below the title's own threads (loading, recording).
                    // SAFETY: a plain call on our own thread.
                    unsafe {
                        use windows::Win32::System::Threading::*;
                        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                    }
                    loop {
                        next().make();
                        if WAITING.fetch_sub(1, Ordering::Relaxed) == 1 {
                            drained();
                        } else {
                            super::library::save(false);
                        }
                    }
                });
            if let Err(e) = spawned {
                tracing::warn!(error = %e, "no pipeline worker");
            } else {
                running = true;
            }
        }
        running
    })
}

/// The most pipelines waiting at once since the workers last ran out.
static PEAK: AtomicU64 = AtomicU64::new(0);

/// The workers ran out of work: logged after a batch of 10 or more, and
/// the cache saved (at most every 20 s).
fn drained() {
    use super::library::{EARLY, LOADED, MADE};
    if PEAK.swap(0, Ordering::Relaxed) >= 10 {
        tracing::info!(
            made = MADE.load(Ordering::Relaxed),
            loaded = LOADED.load(Ordering::Relaxed),
            started_early = EARLY.load(Ordering::Relaxed),
            "host pipelines all made"
        );
    }
    super::library::save(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r4_geometry_wait_is_exact_and_budget_shared() {
        assert_eq!(r4_hashes(" F133E35EFED2A48A, f133, *, xyz, "), ["f133e35efed2a48a"]);
        let mut budget = R4Grace::default();
        let limit = Duration::from_millis(250);
        let spent = budget.reserve(12, limit);
        assert_eq!(spent, limit);
        assert_eq!(budget.reserve(12, limit), Duration::ZERO);
        budget.refund(12, spent, Duration::from_millis(90));
        assert_eq!(budget.reserve(12, limit), Duration::from_millis(160));
        assert_eq!(budget.reserve(13, limit), limit);
        // A completion from the old frame must not credit the new frame.
        budget.refund(12, spent, Duration::ZERO);
        assert_eq!(budget.reserve(12, limit), Duration::ZERO);
        assert_eq!(budget.reserve(13, limit), Duration::ZERO);
    }

    /// A first use queues the pipeline and, within the budget, gets the
    /// worker's answer (here: no host pipeline for no shaders).
    #[test]
    fn matching_live_pipeline_descriptions_share_pending_work() {
        let a = shared(Vec::new(), Some(Box::new(GraphicsState::default())));
        let b = shared(Vec::new(), Some(Box::new(GraphicsState::default())));
        let compute = shared(Vec::new(), None);
        assert!(Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&a, &compute));
        let mut changed = GraphicsState::default();
        changed.topology = windows::Win32::Graphics::Direct3D12::D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
        let different = shared(Vec::new(), Some(Box::new(changed)));
        assert!(!Arc::ptr_eq(&a, &different));
    }

    #[test]
    fn first_use_is_made_by_a_worker() {
        let p = Arc::new(Pending::new(Vec::new(), None));
        assert!(ready(&p).is_none());
        assert_ne!(p.queue_state.load(Ordering::Relaxed), NOT_QUEUED);
        let started = Instant::now();
        while p.made.get().is_none() && started.elapsed() < Duration::from_secs(10) {
            std::thread::yield_now();
        }
        assert!(matches!(p.made.get(), Some(None)));
    }

    #[test]
    fn pending_pipeline_is_recorded_for_the_next_launch() {
        let mut p = Pending::new(Vec::new(), None);
        p.key = "pending-pipeline-test".to_owned();
        // Simulate a worker that has taken the job but cannot finish it yet.
        p.queue_state.store(MAKING, Ordering::Relaxed);
        let p = Arc::new(p);

        assert!(!super::super::library::wanted_early(&p.key));
        assert!(ready(&p).is_none());
        assert!(p.made.get().is_none());
        assert!(super::super::library::wanted_early(&p.key));
    }

    #[test]
    fn builder_preflight_records_even_a_failed_pipeline_for_the_next_launch() {
        let mut p = Pending::new(Vec::new(), None);
        p.key = "builder-preflight-test".to_owned();
        let p = Arc::new(p);
        assert!(!super::super::library::wanted_early(&p.key));
        assert!(!now(&p));
        assert!(matches!(p.made.get(), Some(None)));
        assert!(super::super::library::wanted_early(&p.key));
    }

    #[test]
    fn r4_priority_preserves_requested_first_and_single_claim() {
        assert_eq!(r4_keys(" 0123456789ABCDEF0123456789ABCDEF, f133, * "), ["0123456789abcdef0123456789abcdef"]);
        let old = Arc::new(Pending::new(Vec::new(), None));
        let selected = Arc::new(Pending::new(Vec::new(), None));
        let promoted = Arc::new(Pending::new(Vec::new(), None));
        let draw = Arc::new(Pending::new(Vec::new(), None));
        let mut queue = Queue::new();
        assert_eq!(queue.push(&old, EARLY), Some(true));
        assert_eq!(queue.push(&selected, EARLY), Some(true));
        assert_eq!(queue.push(&selected, R4_EARLY), Some(false));
        assert_eq!(queue.push(&promoted, R4_EARLY), Some(true));
        assert_eq!(queue.push(&promoted, REQUESTED), Some(false));
        assert_eq!(queue.push(&draw, REQUESTED), Some(true));
        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &promoted));
        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &draw));
        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &selected));
        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &old));
        assert!(queue.pop().is_none());
    }

    #[test]
    fn draws_promote_early_pipelines_without_claiming_them_twice() {
        let earlier = Arc::new(Pending::new(Vec::new(), None));
        let promoted = Arc::new(Pending::new(Vec::new(), None));
        let requested = Arc::new(Pending::new(Vec::new(), None));
        let mut queue = Queue::new();

        assert_eq!(queue.push(&earlier, EARLY), Some(true));
        assert_eq!(queue.push(&promoted, EARLY), Some(true));
        assert_eq!(queue.push(&requested, REQUESTED), Some(true));
        assert_eq!(queue.push(&promoted, REQUESTED), Some(false));
        assert_eq!(queue.push(&promoted, REQUESTED), None);

        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &requested));
        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &promoted));
        assert_eq!(queue.push(&promoted, REQUESTED), None);
        assert!(Arc::ptr_eq(&queue.pop().unwrap(), &earlier));
        assert!(queue.pop().is_none());
    }
}
