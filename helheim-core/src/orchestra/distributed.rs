use crossbeam_queue::SegQueue;
use std::sync::atomic::{AtomicU64, Ordering};
use dashmap::DashMap;
use serde::{Serialize, Deserialize};

// Workaround for LiteralValue parsing. We store string representations 
// or basic types since CodeTaal LiteralValue is tied to helheim_lang.
// We will use String representation of values to keep it simple across nodes.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub name: String,
    pub value: String,
    pub lamport: u64,
    pub source_node: String,
}

/// Globale shared state met Lamport + lock-free delta queue
pub struct DistributedMemory {
    pub globals: DashMap<String, (String, u64)>,   // value + lamport
    pub clock: AtomicU64,
    pub pending_deltas: SegQueue<StateDelta>,            // lock-free outbound
    pub node_id: String,
}

impl DistributedMemory {
    pub fn new(node_id: String) -> Self {
        Self {
            globals: DashMap::new(),
            clock: AtomicU64::new(0),
            pending_deltas: SegQueue::new(),
            node_id,
        }
    }

    #[inline]
    pub fn lamport(&self) -> u64 {
        self.clock.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn bump(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Lokale mutatie (hot path)
    pub fn set_global(&self, name: &str, value: String) {
        let ts = self.bump();
        self.globals.insert(name.to_string(), (value.clone(), ts));

        // Enqueue delta voor latere broadcast (na Concurrent of expliciet)
        self.pending_deltas.push(StateDelta {
            name: name.to_string(),
            value,
            lamport: ts,
            source_node: self.node_id.clone(),
        });
    }

    /// Pas inkomende delta toe (read path)
    pub fn apply_delta(&self, delta: StateDelta) {
        self.globals.entry(delta.name.clone())
            .and_modify(|(v, ts)| {
                if delta.lamport > *ts {
                    *v = delta.value.clone();
                    *ts = delta.lamport;
                }
            })
            .or_insert((delta.value.clone(), delta.lamport));
            
        // Update lokale clock
        let mut c = self.clock.load(Ordering::Relaxed);
        while delta.lamport > c {
            match self.clock.compare_exchange_weak(c, delta.lamport, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => c = actual,
            }
        }
    }

    /// Flush alle pending deltas naar peers (wordt aangeroepen na een Concurrent blok)
    pub fn flush_deltas(&self) -> Vec<StateDelta> {
        let mut out = Vec::new();
        while let Some(d) = self.pending_deltas.pop() {
            out.push(d);
        }
        out
    }
}
