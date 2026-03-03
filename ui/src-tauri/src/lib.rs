#![forbid(unsafe_code)]

use std::sync::{Arc, RwLock, atomic::AtomicU64};

use nohash::{BuildNoHashHasher, IntMap};
use tauri::ipc::InvokeBody;
use transport::SecureTransport;
use ui_ixchg::{ArchivedSendRequest, rkyv::{self, rancor}};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

static CONNECTION_CTR: AtomicU64 = AtomicU64::new(0);
static CONNECTIONS: RwLock<IntMap<u64, Arc<SecureTransport>>> = RwLock::new(IntMap::with_hasher(BuildNoHashHasher::new()));

#[tauri::command]
async fn dispatch_np(req: tauri::ipc::Request<'_>) -> Result<usize, String> {
    match req.body() {
        InvokeBody::Json(_) => Err("expected raw".into()),
        InvokeBody::Raw(req) => {
            let req = rkyv::access::<ArchivedSendRequest, rancor::Error>(&req).map_err(|e| e.to_string())?;
            let connections = CONNECTIONS.read().map_err(|e| e.to_string())?;

            let con = connections.get(&req.id.to_native()).ok_or_else(|| "no id".to_owned())?;

            let n = con.send(&req.data).await.map_err(|e| e.to_string())?;
            
            Ok(n)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
