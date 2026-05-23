pub use rkyv;

use rkyv::{Archive, Serialize};

#[derive(Archive, Serialize)]
pub struct SendRequest {
    pub id: u64,
    pub data: Vec<u8>
}