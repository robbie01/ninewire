pub use rkyv;

use rkyv::{Archive, Serialize, rancor};

#[derive(Archive, Serialize)]
pub struct SendRequest {
    id: u64,
    data: Vec<u8>
}



pub fn bruh() {
    let v = rkyv::access::<ArchivedSendRequest, rancor::BoxedError>(&[]).unwrap();
    let z = &v.data;
}
