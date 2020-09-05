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
    let (_rootfs, root_dentry) = REGISTERED_FS.lock().mount_fs(FSType::RAMFS, "".into());

    REGISTERED_FS.lock().set_root(&root_dentry);
    println!("[REGISTERED_FS]: {}", *REGISTERED_FS.lock());
    println!("[root]: {}", *REGISTERED_FS.lock().get_root().read());
    test_vfs_lookup("/");
    test_vfs_mkdir("/abc");
    test_vfs_mkdir("/abc/test_dir");
    test_vfs_mkdir("/abc/test_dir2");
    test_vfs_lookup("/");
    test_vfs_lookup("/abc"); // Error
    test_vfs_lookup("/abc/test_dir");
    test_vfs_lookup("/abc/test_dir2");
}

fn test_vfs_lookup(path: &str) {
    println!(
        "[vfs_lookup ({})]: {}",
        path,
        *REGISTERED_FS.lock().vfs_lookup(path).unwrap().read()
    );
}

fn test_vfs_mkdir(path: &str) {
    println!(
        "[vfs_mkdir ({})]: {}",
        path,
        *REGISTERED_FS.lock().vfs_mkdir(path).unwrap().read()
    );
}

lazy_static! {
    pub static ref REGISTERED_FS: Mutex<vfs::RegisteredFS> = Mutex::new(RegisteredFS::new());
}
