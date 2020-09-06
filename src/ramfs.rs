use super::*;
use vfs::*;

#[derive(new)]
pub struct RamFS {
    #[new(default)]
    pub blocksize: usize,
    #[new(default)]
    max_inode: Mutex<usize>,
    #[new(default)]
    root: Weak<RamFSINodeLocked>,
    #[new(default)]
    inodes: BTreeMap<usize, Arc<RamFSINodeLocked>>, /* inode cache */
    #[new(default)]
    data: BTreeMap<usize, NodeData>, /* persistent data */
}

impl RamFS {
    pub fn mount(_: &str) -> (FSRef, DentryRef) {
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
        let dentry = root_inner.create_dentry(&root_inner, None, "/");
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
            *locked += 1;
            *locked
        };
        let inode = Arc::new(RamFSINodeLocked(RwLock::new(RamFSINode::new(
            ino,
            Arc::downgrade(&fs_ref),
        ))));
        let mut fsw = self.0.write();
        fsw.inodes.insert(ino, inode.clone());
        fsw.data.insert(
            ino,
            NodeData {
                metadata: {
                    let mut md = metadata.unwrap_or(Default::default());
                    md.ino = ino;
                    md
                },
                ..Default::default()
            },
        );
        Ok(inode)
    }

    fn link_inode(&self, parent: &INodeRef, sub: &INodeRef, name: &str) {
        let sub_ino = sub.get_metadata().ino;
        let parent_ino = parent.get_metadata().ino;
        let mut fs = self.0.write();
        let parent_data = fs.data.get_mut(&parent_ino).unwrap();
        parent_data.children_ino.insert(String::from(name), sub_ino);
        parent_data.metadata.nlink += 1;
        let sub_data = fs.data.get_mut(&sub_ino).unwrap();
        sub_data.metadata.nlink += 1;
    }
    fn get_inode(&self, fs_ref: &Arc<Self>, ino: usize) -> Result<Arc<RamFSINodeLocked>> {
        let mut fs = self.0.write();
        if let Some(inode) = fs.inodes.get(&ino) {
            return Ok(inode.clone());
        }
        if let Some(_node_data) = fs.data.get(&ino) {
            return {
                let inode = Arc::new(RamFSINodeLocked(RwLock::new(RamFSINode::new(
                    ino,
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
    // fn get_node_data_mut(&self) -> NodeData {
    //     let fs = self.get_fs_special();
    //     let fs = fs.0.read();
    //     let ino = self.0.read().ino;
    //     let data = fs.data.get(&ino).unwrap();
    //     data.clone()
    // }
    fn get_fs_special(&self) -> Arc<RamFSLocked> {
        return self.0.read().fs.upgrade().unwrap();
    }

    fn create_dentry(
        &self,
        self_ref: &Arc<RamFSINodeLocked>,
        parent: Option<DentryRef>,
        name: &str, /* `name` will not be used when `parent` is `None `*/
    ) -> DentryRef {
        let dentry = Arc::new(RwLock::new(Dentry {
            parent: if let Some(parent) = &parent {
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
        if let Some(parent) = parent {
            parent
                .write()
                .subdirs
                .insert(String::from(name), Arc::downgrade(&dentry));
        }
        self.0.write().dentries.push(dentry.clone());

        return dentry;
    }

    fn create_entity(
        &self,
        dentry: &DentryRef,
        name: &str,
        _: usize,
        mode: INodeType,
    ) -> Result<DentryRef> {
        let fs = self.get_fs_special();
        let inode = fs
            .alloc_inode(
                &fs,
                Some(INodeMetaData {
                    mode: mode,
                    ..Default::default()
                }),
            )
            .unwrap();
        fs.link_inode(
            &dentry.read().inode.upgrade().unwrap(),
            &{ inode.clone() },
            name,
        );
        let dentry = inode.create_dentry(&inode, Some(dentry.clone()), name);
        Ok(dentry)
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
        match node_data.children_ino.get(name) {
            Some(ino) => {
                let fs = self.get_fs_special();
                let inode = fs.get_inode(&fs, *ino)?;
                let dentry = inode.create_dentry(&inode, Some(dir.clone()), name);
                return Ok(dentry);
            }
            None => Err(Error::new(ENOENT)),
        }
    }

    fn mkdir(&self, dentry: &DentryRef, name: &str, flag: usize) -> Result<DentryRef> {
        self.create_entity(dentry, name, flag, INodeType::IFDIR)
    }

    fn unlink(&self, dentry: &DentryRef, name: &str, target: &DentryRef, flag: usize) -> Result<()> {
        let fs = self.get_fs_special();
        let mut fsw = fs.0.write();
        let node_data = fsw
            .data
            .get_mut(&self.0.read().ino)
            .ok_or_else(|| Error::new(ENOENT))?;
        node_data
            .children_ino
            .remove(name)
            .ok_or_else(|| Error::new(ENOENT))?;
        dentry.write().subdirs.remove(name);
        Ok(())
    }

    fn create(&self, dentry: &DentryRef, name: &str, flag: usize) -> Result<DentryRef> {
        self.create_entity(dentry, name, flag, INodeType::IFREG)
    }

    fn readdir_inodes(&self, dentry: &DentryRef, flag: usize) -> Result<BTreeMap<String, usize>> {
        let fs = self.get_fs_special();
        let fsr = fs.0.write();
        let node_data = fsr
            .data
            .get(&self.0.read().ino)
            .ok_or_else(|| Error::new(ENOENT))?;
        Ok(node_data.children_ino.clone())
    }

    // fn rename(
    //     &self,
    //     dentry: &DentryRef,
    //     name: &str,
    //     new_name: &str,
    //     flag: usize,
    // ) -> Result<()> {
    //     let fs = self.get_fs_special();
    //     let mut fsw = fs.0.write();
    //     let node_data = fsw.data.get_mut(&self.0.read().ino).ok_or_else(|| Error::new(ENOENT))?;
    //     let sub_entity_ino =  node_data.children_ino.remove(name).ok_or_else(|| Error::new(ENOENT))?;
    //     node_data.children_ino.insert(new_name.to_string(), sub_entity_ino);
    //     if let Some(sub_entity_dentry) = dentry.write().subdirs.remove(name) {
    //         dentry.write().subdirs.insert(new_name.to_string(), sub_entity_dentry);
    //     };
    //     Ok(())
    // }
}
