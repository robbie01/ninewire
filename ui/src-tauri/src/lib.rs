#![forbid(unsafe_code)]

use std::{cell::Cell, collections::BTreeMap, rc::Rc, sync::Arc};

use bytes::Bytes;
use tauri::{async_runtime::RwLock, ipc::InvokeBody};
use transport::NpTransport;
use ui_ixchg::{ArchivedSendRequest, rkyv::{self, rancor}};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

thread_local! {    
    #[allow(dead_code)]
    static CONNECTION_CTR: Cell<u64> = const { Cell::new(0) };
    #[allow(clippy::arc_with_non_send_sync)]
    static CONNECTIONS: Arc<RwLock<BTreeMap<u64, Rc<dyn NpTransport>>>> = Arc::new(RwLock::const_new(BTreeMap::new()));
}

#[tauri::command]
#[allow(dead_code)]
async fn connect_np(_req: tauri::ipc::Request<'_>) -> Result<u64, String> {
    todo!()
}

#[tauri::command]
#[allow(dead_code)]
async fn dispatch_np(req: tauri::ipc::Request<'_>) -> Result<(), String> {
    match req.body() {
        InvokeBody::Json(_) => Err("expected raw".into()),
        InvokeBody::Raw(req) => {
            let req = rkyv::access::<ArchivedSendRequest, rancor::Error>(req).map_err(|e| e.to_string())?;
            let connections = CONNECTIONS.with(|c| c.clone().read_owned()).await;

            let con = connections.get(&req.id.to_native()).ok_or_else(|| "no id".to_owned())?;

            con.send(Bytes::copy_from_slice(&req.data)).await.map_err(|e| e.to_string())?;
            
            Ok(())
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
