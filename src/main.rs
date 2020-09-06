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

lazy_static! {
    pub static ref REGISTERED_FS: Mutex<vfs::RegisteredFS> = Mutex::new(RegisteredFS::new());
}

fn main() {
    println!("run `cargo test` instead");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(unused_must_use)]
    #[test]
    fn test() {
        REGISTERED_FS
            .lock()
            .register_fs(FSType::RAMFS, ramfs::RamFS::mount);
        let (_rootfs, root_dentry) = REGISTERED_FS.lock().mount_fs(FSType::RAMFS, "".into());

        REGISTERED_FS.lock().set_root(&root_dentry);
        println!("[REGISTERED_FS]: {}", *REGISTERED_FS.lock());
        println!("[root]: {}", *REGISTERED_FS.lock().get_root().read());

        // test for vfs_mkdir
        assert_eq!(test_vfs_lookup("/"), Ok(()));
        assert_eq!(test_vfs_mkdir("/"), Err(Error::new(EEXIST)));
        assert_eq!(test_vfs_mkdir("/abc/test_dir"), Err(Error::new(ENOENT)));
        assert_eq!(test_vfs_mkdir("/abc"), Ok(()));
        assert_eq!(test_vfs_mkdir("/abc/test_dir"), Ok(()));
        assert_eq!(test_vfs_mkdir("/abc/test_dir2"), Ok(()));
        assert_eq!(test_vfs_mkdir("/abc/test_dir3"), Ok(()));

        // test for vfs_lookup
        assert_eq!(test_vfs_lookup("/"), Ok(()));
        assert_eq!(test_vfs_lookup("/abc"), Ok(()));
        assert_eq!(test_vfs_lookup("/abc/test_dir"), Ok(()));
        assert_eq!(test_vfs_lookup("/abc/test_dir2"), Ok(()));

        // test for vfs_create
        assert_eq!(test_vfs_lookup("/test_file"), Err(Error::new(ENOENT)));
        assert_eq!(test_vfs_create("/test_file"), Ok(()));
        assert_eq!(test_vfs_lookup("/test_file"), Ok(()));
        assert_eq!(test_vfs_create("/"), Err(Error::new(EISDIR)));
        assert_eq!(test_vfs_create("/dir/"), Err(Error::new(EISDIR)));
        assert_eq!(test_vfs_create("/test_file"), Err(Error::new(EEXIST)));
        assert_eq!(test_vfs_create("/test_file_2"), Ok(()));

        // test for vfs_open
        let file = test_vfs_open("/test_file", FileMode::O_RDWR);
        assert!(file.is_ok());

        // test for vfs_close
        assert_eq!(test_vfs_close(&mut file.unwrap()), Ok(()));
        
        // test for vfs_rmdir
        assert_eq!(test_vfs_unlink("/"), Err(Error::new(EINVAL)));
        assert_eq!(test_vfs_unlink("/abc"), Err(Error::new(ENOTEMPTY)));
        assert_eq!(test_vfs_unlink("/abc/"), Err(Error::new(ENOTEMPTY)));
        assert_eq!(test_vfs_unlink("/abc/test_dir3"), Ok(()));
        assert_eq!(test_vfs_unlink("/abc/test_dir3"), Err(Error::new(ENOENT)));
        assert_eq!(test_vfs_unlink("/abc/test_dir3"), Err(Error::new(ENOENT)));
        let file = test_vfs_open("/test_file_2", FileMode::O_RDWR);
        assert!(file.is_ok());
        assert_eq!(test_vfs_unlink("/test_file_2"), Err(Error::new(EBUSY)));
        assert_eq!(test_vfs_close(&mut file.unwrap()), Ok(()));
        assert_eq!(test_vfs_unlink("/test_file_2"), Ok(()));
    }

    fn test_vfs_lookup(path: &str) -> Result<()> {
        println!(
            "[vfs_lookup ({})]: {}",
            path,
            *REGISTERED_FS.lock().vfs_lookup(path)?.read()
        );
        Ok(())
    }

    fn test_vfs_mkdir(path: &str) -> Result<()> {
        println!(
            "[vfs_mkdir ({})]: {}",
            path,
            *REGISTERED_FS.lock().vfs_mkdir(path)?.read()
        );
        Ok(())
    }
    fn test_vfs_unlink(path: &str) -> Result<()> {
        REGISTERED_FS.lock().vfs_unlink(path)?;
        println!("[vfs_unlink ({})]", path,);
        Ok(())
    }

    fn test_vfs_create(path: &str) -> Result<()> {
        println!(
            "[vfs_create ({})]: {}",
            path,
            *REGISTERED_FS.lock().vfs_create(path)?.read()
        );
        Ok(())
    }
    fn test_vfs_open(path: &str, mode: FileMode) -> Result<FileRef> {
        let file = REGISTERED_FS.lock().vfs_open(path, mode)?;
        println!("[vfs_open ({})]: {}", path, *file.read());
        Ok(file)
    }

    fn test_vfs_close(file: &mut FileRef) -> Result<()> {
        REGISTERED_FS.lock().vfs_close(file)?;
        println!("[vfs_close ({})]", *file.read());
        Ok(())
    }
}
