use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use nethop_subscription::SourceId;
use nethopd::{
    CandidatePoolNode, NodeAttribution, StableNodeId, SubscriptionMode, build_candidate_pools,
};

struct CountingAllocator;

static CURRENT_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        CURRENT_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, new_size) };
        if !next.is_null() {
            if new_size >= layout.size() {
                record_alloc(new_size - layout.size());
            } else {
                CURRENT_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        next
    }
}

fn record_alloc(size: usize) {
    let current = CURRENT_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    let mut peak = PEAK_BYTES.load(Ordering::Relaxed);
    while current > peak {
        match PEAK_BYTES.compare_exchange_weak(peak, current, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(observed) => peak = observed,
        }
    }
}

fn source(number: usize) -> SourceId {
    SourceId::new(format!("src_{number:032x}")).unwrap()
}

fn node(number: usize, sources: &[SourceId]) -> CandidatePoolNode {
    CandidatePoolNode::new(
        StableNodeId::new(format!("nh1s-{number:016x}")).unwrap(),
        NodeAttribution::new(sources.iter().cloned()).unwrap(),
    )
}

fn benchmark(
    name: &str,
    mode: SubscriptionMode,
    sources: &[SourceId],
    nodes: &[CandidatePoolNode],
    max_candidates: usize,
) -> (String, Duration, usize) {
    let mut durations = Vec::with_capacity(3);
    let mut peak_delta = 0;
    for _ in 0..3 {
        let baseline = CURRENT_BYTES.load(Ordering::Relaxed);
        PEAK_BYTES.store(baseline, Ordering::Relaxed);
        let started = Instant::now();
        let result =
            black_box(build_candidate_pools(mode, sources, nodes, max_candidates).unwrap());
        durations.push(started.elapsed());
        let peak = PEAK_BYTES.load(Ordering::Relaxed);
        peak_delta = peak_delta.max(peak.saturating_sub(baseline));
        black_box(result);
    }
    durations.sort_unstable();
    (name.to_owned(), durations[2], peak_delta)
}

fn main() {
    let sources = (1..=16).map(source).collect::<Vec<_>>();
    let single = (0..10_000)
        .map(|index| node(index, &sources[..1]))
        .collect::<Vec<_>>();
    let uniform = (0..10_000)
        .map(|index| node(index, &[sources[index % 16].clone()]))
        .collect::<Vec<_>>();
    let skewed = (0..10_000)
        .map(|index| {
            let source_index = if index < 9_000 { 0 } else { index % 16 };
            node(index, &[sources[source_index].clone()])
        })
        .collect::<Vec<_>>();
    let duplicated = (0..5_000)
        .map(|index| node(index, &[sources[0].clone(), sources[1].clone()]))
        .chain((5_000..10_000).map(|index| node(index, &[sources[index % 16].clone()])))
        .collect::<Vec<_>>();

    let cases = [
        benchmark(
            "single-1x10000",
            SubscriptionMode::Single,
            &sources[..1],
            &single,
            64,
        ),
        benchmark(
            "merge-16-uniform",
            SubscriptionMode::Merge,
            &sources,
            &uniform,
            64,
        ),
        benchmark(
            "merge-skewed",
            SubscriptionMode::Merge,
            &sources,
            &skewed,
            64,
        ),
        benchmark(
            "merge-50-percent-attribution",
            SubscriptionMode::Merge,
            &sources,
            &duplicated,
            64,
        ),
        benchmark(
            "merge-boundary-256",
            SubscriptionMode::Merge,
            &sources,
            &uniform,
            256,
        ),
    ];

    for (name, p95, peak) in cases {
        println!(
            "{{\"case\":\"{name}\",\"p95_ms\":{},\"peak_extra_bytes\":{peak}}}",
            p95.as_secs_f64() * 1000.0
        );
        assert!(p95 <= Duration::from_millis(10), "{name} exceeded 10ms");
        assert!(peak <= 4 * 1024 * 1024, "{name} exceeded 4MiB");
    }
}
