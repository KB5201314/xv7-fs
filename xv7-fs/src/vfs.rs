use alloc::string::String;
use core::fmt::Debug;
use core::ptr;
use core::str;
use derive_more::Display;

use crate::alloc::string::ToString;
use alloc::collections::btree_map::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use derive_new::new;
use spin::RwLock;
use usyscall::error::*;
use usyscall::fs::*;
use Option::*;

pub type FSMountFunc = fn(&str) -> (FSRef, DentryRef);
pub type FSRef = Arc<dyn FileSystem>;
pub type FSWeakRef = Weak<dyn FileSystem>;

pub type DentryRef = Arc<RwLock<Dentry>>;
pub type DentryWeakRef = Weak<RwLock<Dentry>>;

pub type INodeRef = Arc<dyn INode>;
pub type INodeWeakRef = Weak<dyn INode>;

pub type FileRef = Arc<RwLock<File>>;

#[derive(Debug, Display, PartialEq, Clone, Eq, Ord, PartialOrd, Copy)]
pub enum FSType {
    RAMFS,
}

#[derive(Default)]
pub struct RegisteredFS {
    mount_infos: BTreeMap<FSType, (FSMountFunc, Vec<FSRef>)>,
    root_dentry: Option<DentryRef>,
    opened_files: Vec<FileRef>,
}

#[allow(unused_must_use)]
impl fmt::Display for RegisteredFS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RegisteredFS info: \n");
        for info in &self.mount_infos {
            write!(f, "type: {} mount_times: {}", info.0, (info.1).1.len());
        }
        Ok(())
    }
}

unsafe impl Send for RegisteredFS {}

impl RegisteredFS {
    pub fn new() -> RegisteredFS {
        RegisteredFS {
            ..Default::default()
        }
    }
    pub fn register_fs(&mut self, fstype: FSType, fs_mount: FSMountFunc) {
        self.mount_infos
            .insert(fstype, (fs_mount, Default::default()));
    }
    pub fn mount_fs(&mut self, fstype: FSType, dev_name: &str) -> (FSRef, DentryRef) {
        if let Some((mount, mounted_fss)) = self.mount_infos.get_mut(&fstype) {
            // fake mount
            let result = mount(dev_name);
            mounted_fss.push(result.0.clone());
            result
        } else {
            panic!("filesystem not found: {}", fstype);
        }
    }
    pub fn set_root(&mut self, dentry: &DentryRef) {
        self.root_dentry = Some(dentry.clone());
    }
    pub fn get_root(&mut self) -> DentryRef {
        if self.root_dentry.is_none() {
            panic!("rootfs was not set!")
        }
        self.root_dentry.as_ref().unwrap().clone()
    }
    fn path_lookup<'a>(&mut self, path: &'a str, flags: LookupFlag) -> Result<NameIData<'a>> {
        let mut nd = self.path_init(path, flags);
        self.path_walk(&mut nd, flags)?;
        if nd.cur_ind < nd.paths.len() {
            // `path` may be '/'
            if !flags.contains(LookupFlag::LOOKUP_PARENT) {
                self.lookup_last(&mut nd, flags)?;
            }
        }
        Ok(nd)
    }

    fn path_init<'a>(&mut self, path: &'a str, _flags: LookupFlag) -> NameIData<'a> {
        if path.starts_with('/') {
            let root = self.get_root();
            let current = root.clone();
            NameIData {
                current: current,
                root: root,
                paths: path.split('/').filter(|s| *s != "").collect(),
                cur_ind: 0,
            }
        } else {
            todo!();
        }
    }

    fn path_walk(&mut self, nd: &mut NameIData, flags: LookupFlag) -> Result<()> {
        let cur_inode = nd.current.read().get_inode()?;

        if cur_inode.get_metadata().mode != INodeType::IFDIR {
            return Err(Error::new(ENOTDIR));
        }
        while nd.cur_ind + 1 < nd.paths.len() {
            self.walk_component(nd, flags)?;
        }
        return Ok(());
    }

    fn lookup_last(&mut self, nd: &mut NameIData, flags: LookupFlag) -> Result<()> {
        let dentry = self.lookup_at(nd.paths[nd.cur_ind], &nd.current, flags)?;
        if flags.contains(LookupFlag::LOOKUP_DIRECTORY) {
            match dentry.read().inode.upgrade() {
                Some(inode) => {
                    if inode.get_metadata().mode != INodeType::IFDIR {
                        return Err(Error::new(ENOTDIR));
                    }
                    Ok(())
                }
                None => Err(Error::new(ENOENT)),
            }?;
        }
        nd.cur_ind += 1;
        nd.current = dentry.clone();
        return Ok(());
    }

    fn walk_component(&mut self, nd: &mut NameIData, flags: LookupFlag) -> Result<()> {
        let dentry = self.lookup_at(nd.paths[nd.cur_ind], &nd.current, flags)?;
        let nexti = dentry.read().inode.upgrade();
        if nexti.is_none() {
            return Err(Error::new(ENOENT));
        }
        if nexti.unwrap().get_metadata().mode != INodeType::IFDIR {
            Err(Error::new(ENOTDIR))
        } else {
            nd.cur_ind += 1;
            nd.current = dentry.clone();
            Ok(())
        }
    }

    fn lookup_at(
        &mut self,
        name: &str,
        current: &DentryRef,
        flags: LookupFlag,
    ) -> Result<DentryRef> {
        Ok({
            let current_inode = current.read().inode.clone();
            if !flags.contains(LookupFlag::LOOKUP_REVAL) {
                if let Some(dentry) = current.read().subdirs.get(name) {
                    return Ok(dentry.upgrade().unwrap());
                }
            }
            current_inode
                .upgrade()
                .unwrap()
                .lookup(current, name)?
                .clone()
        })
    }

    pub fn vfs_lookup(&mut self, path: &str) -> Result<DentryRef> {
        self.path_lookup(path, LookupFlag::empty())
            .map(|nd| nd.current)
    }
    pub fn vfs_mkdir(&mut self, path: &str) -> Result<DentryRef> {
        let mut nd = self.path_lookup(path, LookupFlag::LOOKUP_PARENT)?;
        /* if path equals to `/` or the target exist */
        if nd.paths.len() == 0 || self.lookup_last(&mut nd, LookupFlag::empty()).is_ok() {
            Err(Error::new(EEXIST))
        } else {
            let parent = nd.current.clone();
            let parent_inode = parent.read().get_inode()?;
            parent_inode.mkdir(&parent, nd.paths[nd.cur_ind])
        }
    }
    pub fn vfs_unlink(&mut self, path: &str) -> Result<()> {
        let mut nd = self.path_lookup(path, LookupFlag::LOOKUP_PARENT)?;
        if nd.paths.len() == 0 {
            /* if path equals to `/` */
            return Err(Error::new(EINVAL));
        }
        let parent = nd.current.clone();
        self.lookup_last(&mut nd, LookupFlag::empty())?;
        let current_inode = nd.current.read().get_inode()?;
        /* at least for now，you cannot delete a file which is opened by a process */
        for file in &self.opened_files {
            if ptr::eq(current_inode.as_ref(), file.read().inode.as_ref()) {
                return Err(Error::new(EBUSY));
            }
        }
        /* if delete directory, it must be empty first */
        if current_inode.get_metadata().mode == INodeType::IFDIR {
            let inodes = current_inode.readdir_inodes(&nd.current)?;
            if inodes.len() != 0 {
                return Err(Error::new(ENOTEMPTY));
            }
        }
        let parent_inode = parent.read().get_inode()?;
        parent_inode.unlink(&parent, nd.paths[nd.cur_ind - 1])
    }
    pub fn vfs_create(&mut self, path: &str) -> Result<DentryRef> {
        if path.ends_with("/") {
            return Err(Error::new(EISDIR));
        }
        let mut nd = self.path_lookup(path, LookupFlag::LOOKUP_PARENT)?;
        let parent = nd.current.clone();
        if self.lookup_last(&mut nd, LookupFlag::empty()).is_ok() {
            Err(Error::new(EEXIST))
        } else {
            let parent_inode = parent.read().get_inode()?;
            parent_inode.create(&parent, nd.paths[nd.cur_ind])
        }
    }

    pub fn vfs_open(&mut self, path: &str, mode: FileMode) -> Result<FileRef> {
        // TODO: check `mode`
        // TODO: search in self.opened_files
        let nd = self.path_lookup(path, LookupFlag::empty())?;
        let lookup_result = nd.current;
        let inode = lookup_result
            .read()
            .inode
            .upgrade()
            .ok_or_else(|| Error::new(ENOENT))?;
        if mode.contains(FileMode::O_DIRECTORY) {
            if inode.get_metadata().mode != INodeType::IFDIR {
                return Err(Error::new(ENOTDIR));
            }
        }
        let file = Arc::new(RwLock::new(File::new(path.to_string(), 0, 0, inode, mode)));
        self.opened_files.push(file.clone());
        return Ok(file);
    }

    pub fn vfs_close(&mut self, file: &FileRef) -> Result<()> {
        for i in 0..self.opened_files.len() {
            if ptr::eq(file.as_ref(), self.opened_files.get(i).unwrap().as_ref()) {
                self.opened_files.remove(i);
                break;
            }
        }
        Ok(())
    }

    pub fn vfs_write(&mut self, file: &FileRef, buf: &[u8]) -> Result<usize> {
        // TODO: check buf address is safe to read
        /* check write */
        {
            let fr = file.read();
            /* at least for now，you can only call write to a regular file */
            if fr.inode.get_metadata().mode != INodeType::IFREG {
                return Err(Error::new(EINVAL));
            }
            if !(fr.mode.contains(FileMode::O_WRONLY)
                || fr.mode.contains(FileMode::O_RDWR)
                || fr.mode.contains(FileMode::O_APPEND))
            {
                return Err(Error::new(EBADF));
            }
        }
        let inode = file.read().inode.clone();
        inode.write(file, buf)
    }

    pub fn vfs_read(&mut self, file: &FileRef, buf: &mut [u8]) -> Result<usize> {
        // TODO: check buf address is safe to write
        /* check read */
        {
            let fr = file.read();
            /* at least for now，you can only call read to a regular file */
            if fr.inode.get_metadata().mode != INodeType::IFREG {
                return Err(Error::new(EINVAL));
            }
            if !(fr.mode.contains(FileMode::O_RDONLY) || fr.mode.contains(FileMode::O_RDWR)) {
                return Err(Error::new(EBADF));
            }
        }
        let inode = file.read().inode.clone();
        inode.read(file, buf)
    }

    pub fn vfs_readdir(&mut self, file: &FileRef, dirs: &mut [Direntory]) -> Result<usize> {
        // TODO: check dir pointer is safe to write
        /* check read */
        {
            let fr = file.read();
            /* muse be a directory */
            if fr.inode.get_metadata().mode != INodeType::IFDIR {
                return Err(Error::new(EINVAL));
            }
            if !(fr.mode.contains(FileMode::O_RDONLY) || fr.mode.contains(FileMode::O_RDWR)) {
                return Err(Error::new(EBADF));
            }
        }
        let inode = file.read().inode.clone();
        inode.readdir(file, dirs)
    }
    pub fn vfs_stat(&mut self, path: &str, stat: &mut Stat) -> Result<()> {
        let nd = self.path_lookup(path, LookupFlag::empty())?;
        let inode = nd
            .current
            .read()
            .inode
            .upgrade()
            .ok_or_else(|| Error::new(ENOENT))?;
        inode.getattr(&nd.current, stat)
    }
}

struct NameIData<'nd> {
    current: DentryRef,
    root: DentryRef,
    paths: Vec<&'nd str>,
    cur_ind: usize,
}

bitflags! {
struct LookupFlag:u32 {
    const LOOKUP_FOLLOW = 0b00000001;   // follow link (not currently implemented)
    const LOOKUP_DIRECTORY = 0b00000010;// search a directory
    const LOOKUP_PARENT = 0b00000100;   // search the parent and ignore the tail
    const LOOKUP_REVAL = 0b00001000;    // search on fs instead of dentry cache (without test)
}
}
pub trait FileSystem: Send + Sync {
    // fn alloc_inode(&self, fs: &FSRef) -> Result<INodeRef>;
    // fn get_inode(&self, ino: usize) -> Result<INodeRef>;
    fn todo(&self);
}

#[derive(new)]
pub struct Dentry {
    #[new(default)]
    pub parent: DentryWeakRef,
    pub inode: INodeWeakRef,
    // d_op:
    #[new(default)]
    pub subdirs: BTreeMap<String, DentryWeakRef>,
    // d_fsdata: *mut u8,
}

impl Dentry {
    pub fn get_inode(&self) -> Result<INodeRef> {
        self.inode.upgrade().ok_or(Error::new(ENOENT))
    }
}

unsafe impl Send for Dentry {}

impl fmt::Display for Dentry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "inode_of_dentry: {{{:?}}}",
            self.inode.upgrade().unwrap().get_metadata()
        )
    }
}

pub trait INode: Sync + Send {
    fn get_ino(&self) -> usize;
    fn get_metadata(&self) -> INodeMetaData;
    fn set_metadata(&self, metadata: &INodeMetaData);
    fn get_fs(&self) -> FSRef;
    fn get_dentries(&self) -> Vec<DentryRef>;

    // https://elixir.bootlin.com/linux/latest/source/include/linux/fs.h#L1970
    // inode_operations
    fn lookup(&self, dentry: &DentryRef, name: &str) -> Result<DentryRef>;
    //     const char * (*get_link) (struct dentry *, struct inode *, struct delayed_call *);
    //     int (*permission) (struct inode *, int);
    //     struct posix_acl * (*get_acl)(struct inode *, int);
    //     int (*readlink) (struct dentry *, char __user *,int);
    fn create(&self, dentry: &DentryRef, name: &str) -> Result<DentryRef>;
    //     int (*create) (struct inode *,struct dentry *, umode_t, bool);
    //     int (*link) (struct dentry *,struct inode *,struct dentry *);
    fn unlink(&self, dentry: &DentryRef, name: &str) -> Result<()>;
    //     int (*unlink) (struct inode *,struct dentry *);
    //     int (*symlink) (struct inode *,struct dentry *,const char *);
    fn mkdir(&self, dentry: &DentryRef, name: &str) -> Result<DentryRef>;
    //     int (*mkdir) (struct inode *,struct dentry *,umode_t);
    // fn rmdir(&self, dentry: &DentryRef, name: &str, target: &DentryRef) -> Result<()>;
    //     int (*rmdir) (struct inode *,struct dentry *);
    //     int (*mknod) (struct inode *,struct dentry *,umode_t,dev_t);
    // fn rename(&self, dentry: &DentryRef, name: &str, new_name: &str) -> Result<()>;
    //     int (*rename) (struct inode *, struct dentry *,
    //             struct inode *, struct dentry *, unsigned int);
    fn getattr(&self, dentry: &DentryRef, stat: &mut Stat) -> Result<()>;
    //     int (*setattr) (struct dentry *, struct iattr *);
    //     int (*getattr) (const struct path *, struct kstat *, u32, unsigned int);
    //     ssize_t (*listxattr) (struct dentry *, char *, size_t);
    //     int (*fiemap)(struct inode *, struct fiemap_extent_info *, u64 start,
    //               u64 len);
    //     int (*update_time)(struct inode *, struct timespec64 *, int);
    //     int (*atomic_open)(struct inode *, struct dentry *,
    //                struct file *, unsigned open_flag,
    //                umode_t create_mode);
    //     int (*tmpfile) (struct inode *, struct dentry *, umode_t);
    //     int (*set_acl)(struct inode *, struct posix_acl *, int);

    // https://elixir.bootlin.com/linux/latest/source/include/linux/fs.h#L1923
    // struct file_operations
    //     loff_t (*llseek) (struct file *, loff_t, int);
    fn read(&self, file: &FileRef, buf: &mut [u8]) -> Result<usize>;
    //     ssize_t (*read) (struct file *, char __user *, size_t, loff_t *);
    fn write(&self, file: &FileRef, buf: &[u8]) -> Result<usize>;
    //     ssize_t (*write) (struct file *, const char __user *, size_t, loff_t *);
    //     ssize_t (*read_iter) (struct kiocb *, struct iov_iter *);
    //     ssize_t (*write_iter) (struct kiocb *, struct iov_iter *);
    //     int (*iopoll)(struct kiocb *kiocb, bool spin);
    //     int (*iterate) (struct file *, struct dir_context *);
    //     int (*iterate_shared) (struct file *, struct dir_context *);
    //     __poll_t (*poll) (struct file *, struct poll_table_struct *);
    //     long (*unlocked_ioctl) (struct file *, unsigned int, unsigned long);
    //     long (*compat_ioctl) (struct file *, unsigned int, unsigned long);
    //     int (*mmap) (struct file *, struct vm_area_struct *);
    //     unsigned long mmap_supported_flags;
    //     int (*open) (struct inode *, struct file *);
    //     int (*flush) (struct file *, fl_owner_t id);
    //     int (*release) (struct inode *, struct file *);
    //     int (*fsync) (struct file *, loff_t, loff_t, int datasync);
    //     int (*fasync) (int, struct file *, int);
    //     int (*lock) (struct file *, int, struct file_lock *);
    //     ssize_t (*sendpage) (struct file *, struct page *, int, size_t, loff_t *, int);
    //     unsigned long (*get_unmapped_area)(struct file *, unsigned long, unsigned long, unsigned long, unsigned long);
    //     int (*check_flags)(int);
    //     int (*flock) (struct file *, int, struct file_lock *);
    //     ssize_t (*splice_write)(struct pipe_inode_info *, struct file *, loff_t *, size_t, unsigned int);
    //     ssize_t (*splice_read)(struct file *, loff_t *, struct pipe_inode_info *, size_t, unsigned int);
    //     int (*setlease)(struct file *, long, struct file_lock **, void **);
    //     long (*fallocate)(struct file *file, int mode, loff_t offset,
    //               loff_t len);
    //     void (*show_fdinfo)(struct seq_file *m, struct file *f);
    // #ifndef CONFIG_MMU
    //     unsigned (*mmap_capabilities)(struct file *);
    // #endif
    //     ssize_t (*copy_file_range)(struct file *, loff_t, struct file *,
    //             loff_t, size_t, unsigned int);
    //     loff_t (*remap_file_range)(struct file *file_in, loff_t pos_in,
    //                    struct file *file_out, loff_t pos_out,
    //                    loff_t len, unsigned int remap_flags);
    //     int (*fadvise)(struct file *, loff_t, loff_t, int);

    // int (*lseek) (struct inode *, struct file *, off_t, int);
    // int (*read) (struct inode *, struct file *, char *, int);
    // int (*write) (struct inode *, struct file *, const char *, int);
    fn readdir_inodes(&self, dentry: &DentryRef) -> Result<BTreeMap<String, usize>>;
    fn readdir(&self, file: &FileRef, dirs: &mut [Direntory]) -> Result<usize>;
    // int (*readdir) (struct inode *, struct file *, void *, filldir_t);
    // int (*select) (struct inode *, struct file *, int, select_table *);
    // int (*ioctl) (struct inode *, struct file *, unsigned int, unsigned long);
    // int (*mmap) (struct inode *, struct file *, struct vm_area_struct *);
    // int (*open) (struct inode *, struct file *);
    // void (*release) (struct inode *, struct file *);
    // int (*fsync) (struct inode *, struct file *);
    // int (*fasync) (struct inode *, struct file *, int);
    // int (*check_media_change) (kdev_t dev);
    // int (*revalidate) (kdev_t dev);
}

#[derive(new, Clone, Default)]
pub struct INodeMetaData {
    pub mode: INodeType,
    #[new(default)]
    pub uid: usize,
    #[new(default)]
    pub gid: usize,
    #[new(default)]
    pub ino: usize,
    #[new(default)]
    pub atime: usize,
    #[new(default)]
    pub mtime: usize,
    #[new(default)]
    pub ctime: usize,
    // i_fop:
    // i_sb_list_pre: *mut INode,
    // i_sb_list_next: *mut INode,
    #[new(default)]
    pub nlink: usize,
    // i_private: *mut u8,
    #[new(default)]
    pub link: String,
}

impl fmt::Debug for INodeMetaData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ino: {} mode: {:?} nlink: {}",
            self.ino, self.mode, self.nlink
        )
    }
}

#[derive(new)]
pub struct File {
    pub path: String,
    pub pos: usize,
    pub ref_count: usize,
    pub inode: INodeRef,
    pub mode: FileMode,
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "File {{path: {}, pos: {}, ref_count: {}, inode: {:?}, mode: {:?}}}",
            self.path,
            self.pos,
            self.ref_count,
            self.inode.get_metadata(),
            self.mode
        )
    }
}
