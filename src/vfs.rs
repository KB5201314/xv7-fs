use super::*;
use alloc::string::String;
use core::str;
use derive_more::Display;

pub type FSMountFunc = fn(String) -> (FSRef, DentryRef);
pub type FSRef = Arc<dyn FileSystem>;
pub type DentryRef = Arc<RwLock<Dentry>>;
pub type INodeRef = Arc<dyn INode>;
pub type FSWeakRef = Weak<dyn FileSystem>;
pub type DentryWeakRef = Weak<RwLock<Dentry>>;
pub type INodeWeakRef = Weak<dyn INode>;

#[derive(Debug, Display, PartialEq, Clone, Eq, Ord, PartialOrd, Copy)]
pub enum FSType {
    RAMFS,
}

#[derive(Default)]
pub struct RegisteredFS {
    mount_infos: BTreeMap<FSType, (FSMountFunc, Vec<FSRef>)>,
    root_dentry: Option<DentryRef>,
}

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
    pub fn mount_fs(&mut self, fstype: FSType, dev_name: String) -> (FSRef, DentryRef) {
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
    pub fn vfs_lookup(&mut self, path: &str) -> Result<DentryRef> {
        let mut nd = self.path_init(path);
        self.path_walk(&mut nd)?;
        if nd.cur_ind < nd.paths.len() {
            self.lookup_last(&mut nd)?;
        }
        Ok(nd.current)
    }

    fn lookup_last(&mut self, nd: &mut NameIData) -> Result<()> {
        let dentry = self.lookup_at(nd.paths[nd.cur_ind], &nd.current)?;
        return Ok(());
    }

    fn path_walk(&mut self, nd: &mut NameIData) -> Result<()> {
        while nd.cur_ind + 1 < nd.paths.len() {
            self.walk_component(nd)?;
        }
        return Ok(());
    }

    fn lookup_at(&mut self, name: &str, current: &DentryRef) -> Result<DentryRef> {
        Ok({
            let current_inode = &current.read().inode;
            match current.read().subdirs.get(name) {
                Some(dentry) => dentry.upgrade().unwrap(),
                None => current_inode
                    .upgrade()
                    .unwrap()
                    .lookup(current, name, 0)?
                    .clone(),
            }
        })
    }
    fn walk_component(&mut self, nd: &mut NameIData) -> Result<()> {
        let dentry = self.lookup_at(nd.paths[nd.cur_ind], &nd.current)?;
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

    fn path_init<'a>(&mut self, path: &'a str) -> NameIData<'a> {
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
}

pub struct NameIData<'nd> {
    current: DentryRef,
    root: DentryRef,
    paths: Vec<&'nd str>,
    cur_ind: usize,
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

unsafe impl Send for Dentry {}

impl fmt::Display for Dentry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dentry of inode: {}",
            self.inode.upgrade().unwrap().get_metadata()
        )
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum INodeType {
    IFIFO,
    IFCHR,
    IFDIR,
    IFBLK,
    IFREG,
    IFLNK,
    IFSOCK,
}

impl Default for INodeType {
    fn default() -> Self {
        INodeType::IFREG
    }
}

pub trait INode: Sync + Send {
    fn get_metadata(&self) -> INodeMetaData;
    fn set_metadata(&self, metadata: &INodeMetaData);
    fn get_fs(&self) -> FSRef;
    fn get_dentries(&self) -> Vec<DentryRef>;

    // https://elixir.bootlin.com/linux/latest/source/include/linux/fs.h#L1970
    // inode_operations
    fn lookup(&self, dir: &DentryRef, name: &str, flag: usize) -> Result<DentryRef>;
    //     const char * (*get_link) (struct dentry *, struct inode *, struct delayed_call *);
    //     int (*permission) (struct inode *, int);
    //     struct posix_acl * (*get_acl)(struct inode *, int);
    //     int (*readlink) (struct dentry *, char __user *,int);
    //     int (*create) (struct inode *,struct dentry *, umode_t, bool);
    //     int (*link) (struct dentry *,struct inode *,struct dentry *);
    //     int (*unlink) (struct inode *,struct dentry *);
    //     int (*symlink) (struct inode *,struct dentry *,const char *);
    fn mkdir(&self, name: &str, flag: usize) -> Result<()>;
    //     int (*mkdir) (struct inode *,struct dentry *,umode_t);
    //     int (*rmdir) (struct inode *,struct dentry *);
    //     int (*mknod) (struct inode *,struct dentry *,umode_t,dev_t);
    //     int (*rename) (struct inode *, struct dentry *,
    //             struct inode *, struct dentry *, unsigned int);
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
    //     ssize_t (*read) (struct file *, char __user *, size_t, loff_t *);
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
    pub link: &'static str,
}

impl fmt::Display for INodeMetaData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ino: {} mode: {:?} nlink: {}",
            self.ino, self.mode, self.nlink
        )
    }
}
