//! Shared bounded ownership budget for legacy sessions and retained contexts.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, Default)]
struct Counts {
    total: usize,
    mounts: HashMap<String, usize>,
}
#[derive(Debug)]
pub struct ContextBudget {
    counts: Mutex<Counts>,
    per_mount: usize,
}
#[derive(Debug)]
pub struct Reservation {
    budget: Arc<ContextBudget>,
    mount: String,
}
impl ContextBudget {
    pub fn new(per_mount: usize) -> Arc<Self> {
        Arc::new(Self {
            counts: Mutex::new(Counts::default()),
            per_mount,
        })
    }
    pub fn reserve(self: &Arc<Self>, mount: &str) -> Result<Arc<Reservation>, String> {
        let mut counts = self.counts.lock().unwrap();
        if counts.total >= 256 || counts.mounts.get(mount).copied().unwrap_or(0) >= self.per_mount {
            return Err("upstream context limit reached; reset unused retained contexts or close legacy sessions".into());
        }
        counts.total += 1;
        *counts.mounts.entry(mount.into()).or_default() += 1;
        Ok(Arc::new(Reservation {
            budget: self.clone(),
            mount: mount.into(),
        }))
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let mut counts = self.budget.counts.lock().unwrap();
        counts.total -= 1;
        if let Some(count) = counts.mounts.get_mut(&self.mount) {
            *count -= 1;
            if *count == 0 {
                counts.mounts.remove(&self.mount);
            }
        }
    }
}
