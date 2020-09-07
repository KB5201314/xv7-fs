# xv7-fs

Here is an implementation of the file system framework provided to the xv7 system.

## module

- xv7-fs

implementation of vfs.

- xv7-fs-ramfs

memory file system based on vfs.

## TODO list

- inode_operations
    - [x] lookup
    - [x] mkdir
    - [x] create
    - [ ] setattr
    - [x] getattr
    - [ ] update_time
    - [x] unlink

- file_operations
    - [x] read
    - [x] readdir
    - [x] write

- extra syscall
    - [x] open
    - [x] close
    
