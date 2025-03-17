use std::fs::Metadata;

use bytestring::ByteString;
use npwire::{Qid, Stat, DMDIR, QTDIR, QTFILE};

pub mod open;
pub mod path;

fn inode(meta: &Metadata) -> u64 {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            std::os::unix::fs::MetadataExt::ino(meta)
        } else {
            compile_error!("implement inode")
        }
    }
}

fn qid(meta: &Metadata) -> Qid {
    Qid {
        type_: if meta.is_dir() { QTDIR } else { QTFILE },
        version: 0,
        path: inode(meta)
    }
}

fn stat(session: &super::Session, name: &str, meta: &Metadata) -> Stat {
    Stat {
        type_: 0,
        dev: 0,
        qid: qid(meta),
        mode: if meta.is_dir() { DMDIR | 0o555 } else { 0o444 },
        atime: 0,
        mtime: 0,
        length: if meta.is_dir() { 0 } else { meta.len() },
        name: name.into(),
        uid: session.uname.clone(),
        gid: session.uname.clone(),
        muid: session.uname.clone()
    }
}

pub const ROOT_QID: Qid = Qid { type_: QTDIR, version: 0, path: 0 };
pub const RPC_QID: Qid = Qid { type_: QTFILE, version: 0, path: !0 };

fn root_stat(session: &super::Session) -> Stat {
    Stat {
        type_: 0,
        dev: 0,
        qid: ROOT_QID,
        mode: DMDIR | 0o555,
        atime: 0,
        mtime: 0,
        length: 0,
        name: ByteString::from_static("/"),
        uid: session.uname.clone(),
        gid: session.uname.clone(),
        muid: session.uname.clone()
    }
}

fn rpc_stat(session: &super::Session) -> Stat {
    Stat {
        type_: 0,
        dev: 0,
        qid: RPC_QID,
        mode: 0o600,
        atime: 0,
        mtime: 0,
        length: 0,
        name: ByteString::from_static("rpc"),
        uid: session.uname.clone(),
        gid: session.uname.clone(),
        muid: session.uname.clone()
    }
}