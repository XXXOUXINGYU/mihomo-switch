use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex, MutexGuard,
};

use crate::{
    runner::RunnerHandle,
    traffic::{PortTrafficSampler, TrafficMonitor},
};

#[derive(Default)]
pub struct LatencyControl {
    generation: AtomicU64,
    running: AtomicBool,
}

impl LatencyControl {
    pub fn try_begin_run(&self) -> Option<LatencyRunGuard<'_>> {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()?;
        let token = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        Some(LatencyRunGuard {
            control: self,
            token,
        })
    }

    pub fn cancel(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_active(&self, token: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == token
    }
}

pub struct LatencyRunGuard<'a> {
    control: &'a LatencyControl,
    token: u64,
}

impl LatencyRunGuard<'_> {
    pub fn token(&self) -> u64 {
        self.token
    }
}

impl Drop for LatencyRunGuard<'_> {
    fn drop(&mut self) {
        self.control.running.store(false, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub struct AppState {
    pub runner: RunnerHandle,
    pub traffic: TrafficMonitor,
    pub port_traffic: PortTrafficSampler,
    pub latency: LatencyControl,
    settings_operation: Mutex<()>,
}

impl AppState {
    pub fn settings_guard(&self) -> MutexGuard<'_, ()> {
        self.settings_operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
