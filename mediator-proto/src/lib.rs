#![forbid(unsafe_code)]

tonic::include_proto!("ninewire.mediator");

pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("mediator_descriptor");