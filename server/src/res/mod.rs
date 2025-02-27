use std::fs::Metadata;

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

fn stat(name: &str, meta: &Metadata) -> Stat {
    Stat {
        type_: 0,
        dev: 0,
        qid: qid(meta),
        mode: if meta.is_dir() { DMDIR | 0o555 } else { 0o444 },
        atime: 0,
        mtime: 0,
        length: if meta.is_dir() { 0 } else { meta.len() },
        name: name.into(),
        uid: "me".into(),
        gid: "me".into(),
        muid: "me".into()
    }
}