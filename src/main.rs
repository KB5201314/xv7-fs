#![feature(box_syntax)]

#[macro_use]
extern crate bitflags;

pub mod ramfs;
pub mod vfs;
extern crate alloc;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use core::str;
use derive_new::new;
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};
use usyscall::error::*;
use vfs::*;
use Option::*;

fn main() {
    REGISTERED_FS
        .lock()
        .register_fs(FSType::RAMFS, ramfs::RamFS::mount);
    let (rootfs, root_dentry) = REGISTERED_FS.lock().mount_fs(FSType::RAMFS, "".into());

    REGISTERED_FS.lock().set_root(&root_dentry);
    println!("[REGISTERED_FS]: {}", *REGISTERED_FS.lock());
    println!("[root]: {}", *REGISTERED_FS.lock().get_root().read());
    test_vfs_lookup("/");
    test_vfs_lookup("/abc"); // Error
}

fn test_vfs_lookup(path: &str) {
    println!(
        "[vfs_lookup ({})]: {}",
        path,
        *REGISTERED_FS
            .lock()
            .vfs_lookup(path)
            .as_ref()
            .unwrap()
            .read()
    );
}

lazy_static! {
    pub static ref REGISTERED_FS: Mutex<vfs::RegisteredFS> = Mutex::new(RegisteredFS::new());
}
