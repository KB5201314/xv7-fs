use core::str;

pub trait FileSystem {
    pub name: str,
    // pub mount
    // pub kill_sb
    pub next: *mut FileSystem,
    pub fs_supers: *mut SuperBlock,
}

pub struct SuperBlock {
    s_list_pre: *mut SuperBlock,
    s_list_next: *mut SuperBlock,
    s_blocksize: usize,
    s_type: *mut FileSystem,
    // s_op
    s_root: *mut Dentry,
    s_inodes: *mut INode,
    s_instances: *mut SuperBlock,
    // s_mounts
    s_fs_info: *mut u8,
}

pub struct Dentry {
    d_parent: *mut Dentry,
    d_name: str,
    d_inone: *mut INode,
    // d_op:
    d_subdirs: *mut Dentry,
    d_fsdata: *mut u8,
}

enum INodeType {
    S_IFIFO,
    S_IFCHR,
    S_IFDIR,
    S_IFBLK,
    S_IFREG,
    S_IFLNK,
    S_IFSOCK,
}

pub struct INode {
    i_mode: INodeType,
    i_uid: usize,
    i_gid: usize,
    // i_op:
    i_sb: *mut SuperBlock,
    i_ino: usize,
    i_atime: usize,
    i_mtime: usize,
    i_ctime: usize,
    // i_fop:
    i_sb_list_pre: *mut INode,
    i_sb_list_next: *mut INode,
    i_nlink: usize,
    i_private: *mut u8,
    i_link: str,
}
