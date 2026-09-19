//! Opt-in per-plugin tick profiling.
//!
//! The mainloop only reports one aggregate `PLUGINS` duration, which cannot say
//! whether a slow tick is host-side (serializing the engine state, copying
//! buffers into wasm memory) or guest-side (the plugin's own wasm code). Set
//! `BL_PROFILE_PLUGINS=1` to have the plugin manager print a per-plugin
//! breakdown once a second.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Time spent in one plugin (or animation instance) tick, split by phase.
#[derive(Default, Clone, Copy)]
pub struct PluginPhaseTimes {
    /// `EngineState::serialize` plus the `dmx_engine` read lock.
    pub engine_serialize: Duration,
    /// Serializing the serial and UDP collections for this plugin.
    pub io_serialize: Duration,
    /// `Memory::write` calls that copy the buffers into guest memory.
    pub memory_write: Duration,
    /// The guest's own `internal_tick` execution.
    pub wasm_call: Duration,
}

impl PluginPhaseTimes {
    fn add(&mut self, other: &Self) {
        self.engine_serialize += other.engine_serialize;
        self.io_serialize += other.io_serialize;
        self.memory_write += other.memory_write;
        self.wasm_call += other.wasm_call;
    }

    fn total(&self) -> Duration {
        self.engine_serialize + self.io_serialize + self.memory_write + self.wasm_call
    }
}

#[derive(Default, Clone, Copy)]
struct PluginAccumulator {
    times: PluginPhaseTimes,
    ticks: u32,
}

/// Accumulates per-plugin phase times and prints a breakdown once a second.
pub struct PluginProfiler {
    enabled: bool,
    last_report: Instant,
    /// Keyed by plugin id; animation instances are folded into their plugin.
    plugins: BTreeMap<u8, PluginAccumulator>,
    manager_overhead: Duration,
    manager_ticks: u32,
    /// Work done once per tick for all plugins together.
    shared: PluginPhaseTimes,
    /// How many engine snapshots were published, and how many of those
    /// actually differed from the previous one.
    snapshots: u32,
    snapshots_changed: u32,
    snapshot_len: usize,
}

impl PluginProfiler {
    pub fn new() -> Self {
        Self {
            enabled: std::env::var("BL_PROFILE_PLUGINS")
                .map(|value| value != "0" && !value.is_empty())
                .unwrap_or(false),
            last_report: Instant::now(),
            plugins: BTreeMap::new(),
            manager_overhead: Duration::ZERO,
            manager_ticks: 0,
            shared: PluginPhaseTimes::default(),
            snapshots: 0,
            snapshots_changed: 0,
            snapshot_len: 0,
        }
    }

    /// Records the once-per-tick work that is shared by all plugins.
    pub fn record_shared(&mut self, times: &PluginPhaseTimes) {
        if !self.enabled {
            return;
        }
        self.shared.add(times);
    }

    /// Records that an engine snapshot was published, and whether its bytes
    /// differed from the previous one.
    pub fn record_engine_snapshot(&mut self, len: usize, changed: bool) {
        if !self.enabled {
            return;
        }
        self.snapshots += 1;
        self.snapshots_changed += u32::from(changed);
        self.snapshot_len = len;
    }

    pub fn record(&mut self, plugin_id: u8, times: &PluginPhaseTimes) {
        if !self.enabled {
            return;
        }
        let entry = self.plugins.entry(plugin_id).or_default();
        entry.times.add(times);
        entry.ticks += 1;
    }

    /// `total` is the full `PluginManager::tick` duration; whatever it has over
    /// the sum of the recorded phases is manager-level overhead.
    pub fn record_manager_tick(&mut self, total: Duration) {
        if !self.enabled {
            return;
        }
        self.manager_ticks += 1;
        let accounted: Duration = self
            .plugins
            .values()
            .map(|entry| entry.times.total())
            .sum::<Duration>()
            + self.shared.total();
        self.manager_overhead = total.saturating_sub(accounted);
        self.maybe_report(total);
    }

    fn maybe_report(&mut self, last_total: Duration) {
        if self.last_report.elapsed() < Duration::from_secs(1) {
            return;
        }

        let ticks = self.manager_ticks.max(1);
        let mut lines = vec![format!(
            "[PROFILE] {ticks} plugin ticks/s, last tick {last_total:?}, manager overhead {:?}",
            self.manager_overhead
        )];

        lines.push(format!(
            "[PROFILE]   engine snapshots: {}/{} changed, {} bytes",
            self.snapshots_changed, self.snapshots, self.snapshot_len,
        ));
        lines.push(format!(
            "[PROFILE]   shared/tick: engine {:?}, io {:?}",
            self.shared.engine_serialize / ticks,
            self.shared.io_serialize / ticks,
        ));

        let mut totals = PluginPhaseTimes::default();
        for (plugin_id, entry) in &self.plugins {
            totals.add(&entry.times);
            let per_tick = entry.times.total() / entry.ticks.max(1);
            lines.push(format!(
                "[PROFILE]   plugin {plugin_id}: {:?}/tick over {} calls \
                 (engine {:?}, io {:?}, memwrite {:?}, wasm {:?})",
                per_tick,
                entry.ticks,
                entry.times.engine_serialize / entry.ticks.max(1),
                entry.times.io_serialize / entry.ticks.max(1),
                entry.times.memory_write / entry.ticks.max(1),
                entry.times.wasm_call / entry.ticks.max(1),
            ));
        }
        lines.push(format!(
            "[PROFILE]   totals/tick: engine {:?}, io {:?}, memwrite {:?}, wasm {:?}",
            totals.engine_serialize / ticks,
            totals.io_serialize / ticks,
            totals.memory_write / ticks,
            totals.wasm_call / ticks,
        ));

        for line in lines {
            tracing::info!("{line}");
        }

        self.plugins.clear();
        self.shared = PluginPhaseTimes::default();
        self.snapshots = 0;
        self.snapshots_changed = 0;
        self.manager_ticks = 0;
        self.last_report = Instant::now();
    }
}

impl Default for PluginProfiler {
    fn default() -> Self {
        Self::new()
    }
}
