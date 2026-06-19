use std::sync::Arc;
use tokio::sync::Mutex;
use dashmap::DashMap;
use tokio::net::{TcpStream, TcpListener};
use lazy_static::lazy_static;

pub enum Resource {
    TcpListener(Arc<Mutex<TcpListener>>),
    TcpStream(Arc<Mutex<TcpStream>>),
}

lazy_static! {
    pub static ref RESOURCE_TABLE: DashMap<u64, Resource> = DashMap::new();
}

pub fn next_handle_id() -> u64 {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
