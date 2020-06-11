use super::*;
use vfs::*;

#[derive(new)]
pub struct RamFS {
    #[new(default)]
    pub blocksize: usize,
    #[new(default)]
    max_inode: Mutex<usize>,
    #[new(default)]
    inodes: BTreeMap<usize, Arc<RamFSINodeLocked>>,
    #[new(default)]
    root: Weak<RamFSINodeLocked>,
    #[new(default)]
    // persistent data
    data: BTreeMap<usize, NodeData>,
}

impl RamFS {
    pub fn mount(_: String) -> (FSRef, DentryRef) {
        let fs_inner = Arc::new(RamFSLocked(RwLock::new(RamFS::new())));
        let root_inner = fs_inner
            .alloc_inode(
                &fs_inner,
                Some(INodeMetaData {
                    mode: INodeType::IFDIR,
                    ..Default::default()
                }),
            )
            .unwrap();
        fs_inner.0.write().root = Arc::downgrade(&root_inner);
        let dentry = root_inner.create_dentry(&root_inner, None);
        return (fs_inner, dentry);
    }
}

#[derive(new, Clone, Default)]
struct NodeData {
    #[new(default)]
    data: Vec<u8>,
    #[new(default)]
    parent_ino: usize,
    #[new(default)]
    children_ino: BTreeMap<String, usize>,
    #[new(default)]
    metadata: INodeMetaData,
}

pub struct RamFSLocked(RwLock<RamFS>);

impl RamFSLocked {
    fn alloc_inode(
        &self,
        fs_ref: &Arc<Self>,
        metadata: Option<INodeMetaData>,
    ) -> Result<Arc<RamFSINodeLocked>> {
        let ino = {
            let ramfs = self.0.read();
            let mut locked = ramfs.max_inode.lock();
            let ino = *locked;
            *locked += 1;
            ino
        };
        let inode = Arc::new(RamFSINodeLocked(RwLock::new(RamFSINode::new(
            Arc::downgrade(&fs_ref),
        ))));
        let mut fsw = self.0.write();
        fsw.inodes.insert(ino, inode.clone());
        fsw.data.insert(
            ino,
            NodeData {
                metadata: metadata.unwrap_or(Default::default()),
                ..Default::default()
            },
        );
        Ok(inode)
    }

    fn get_inode(&self, fs_ref: &Arc<Self>, ino: usize) -> Result<Arc<RamFSINodeLocked>> {
        let mut fs = self.0.write();
        if let Some(inode) = fs.inodes.get(&ino) {
            return Ok(inode.clone());
        }
        if let Some(node_data) = fs.data.get(&ino) {
            return {
                let inode = Arc::new(RamFSINodeLocked(RwLock::new(RamFSINode::new(
                    Arc::downgrade(&fs_ref),
                ))));
                fs.inodes.insert(ino, inode.clone());
                Ok(inode)
            };
        }
        Err(Error::new(ENOENT))
    }
}
impl FileSystem for RamFSLocked {
    fn todo(&self) {
        todo!()
    }
}

#[derive(new)]
pub struct RamFSINode {
    #[new(default)]
    ino: usize,
    // i_op:
    fs: Weak<RamFSLocked>,
    #[new(default)]
    dentries: Vec<DentryRef>,
}

pub struct RamFSINodeLocked(RwLock<RamFSINode>);

impl RamFSINodeLocked {
    fn get_node_data(&self) -> NodeData {
        let fs = self.get_fs_special();
        let fs = fs.0.read();
        let ino = self.0.read().ino;
        let data = fs.data.get(&ino).unwrap();
        data.clone()
    }
    fn get_fs_special(&self) -> Arc<RamFSLocked> {
        return self.0.read().fs.upgrade().unwrap();
    }
    fn create_dentry(
        &self,
        self_ref: &Arc<RamFSINodeLocked>,
        parent: Option<DentryRef>,
    ) -> DentryRef {
        let dentry = Arc::new(RwLock::new(Dentry {
            parent: if let Some(parent) = parent {
                Arc::downgrade(&parent)
            } else {
                Default::default()
            },
            inode: {
                let self_ref: INodeRef = self_ref.clone();
                Arc::downgrade(&self_ref)
            },
            subdirs: Default::default(),
        }));
        self.0.write().dentries.push(dentry.clone());
        return dentry;
    }
}
impl INode for RamFSINodeLocked {
    fn get_metadata(&self) -> INodeMetaData {
        self.get_node_data().metadata
    }

    fn set_metadata(&self, metadata: &INodeMetaData) {
        let fs = self.get_fs_special();
        let mut fs = fs.0.write();
        let ino = self.0.read().ino;
        let data = fs.data.get_mut(&ino).unwrap();
        data.metadata = metadata.clone();
    }
    fn get_fs(&self) -> FSRef {
        return self.0.read().fs.upgrade().unwrap();
    }
    fn get_dentries(&self) -> Vec<DentryRef> {
        return self.0.read().dentries.clone();
    }

    fn lookup(&self, dir: &DentryRef, name: &str, _: usize) -> Result<DentryRef> {
        let node_data = self.get_node_data();
        if node_data.metadata.mode != INodeType::IFDIR {
            return Err(Error::new(ENOTDIR));
        }
        match node_data.children_ino.get(name) {
            Some(ino) => {
                let fs = self.get_fs_special();
                let inode = fs.get_inode(&fs, *ino)?;
                let dentry = inode.create_dentry(&inode, Some(dir.clone()));
                return Ok(dentry);
            }
            None => Err(Error::new(ENOENT)),
        }
    }
    fn mkdir(&self, name: &str, _: usize) -> Result<()> {
        todo!();
        // let mut node = self.0.write();
        // self.0.read().metadata.sb.read().fstype.alloc_inode();
        // node.subnodes[name] = RamFSINodeInner
    }
}
