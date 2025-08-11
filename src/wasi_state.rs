use std::collections::HashMap;

use crate::wasi_fs::{
    self, wasi::filesystem::types::{
        Advice, DescriptorFlags, DescriptorStat, DescriptorType, DirectoryEntry, ErrorCode,
        Filesize, MetadataHashValue, NewTimestamp, OpenFlags, PathFlags,
    }, Descriptor, FsError, FsResult, ReaddirIterator
};
use anyhow::Context as _;
use gix::{dir::Entry, objs::tree::EntryKind, ObjectId, Repository};
use wasmtime::component::{HasData, Linker, Resource};
use wasmtime_wasi::{
    ResourceTable,
    p2::{WasiCtx, WasiView},
};
use wasmtime_wasi_io::IoView;

pub struct WasiState {
    pub wasi_ctx: WasiCtx,
    // This is basically a `Vec<any>`.
    pub resource_table: ResourceTable,
    // The git filesystem.
    pub gitfs: GitFs,
}

impl WasiView for WasiState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }
}

impl IoView for WasiState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.resource_table
    }
}

pub struct GitFs {
    // Git repository.
    pub repo: Repository,
    // Root tree object ID.
    pub root: ObjectId,
    // Blob contents. When we read a blob it goes into here.
    // When we support writing we can modify them here too.
    // There's no garbage collection currently - if you open a file, read
    // it and then close it, it will stay here. This would be relatively easy
    // to fix with a reference count.
    pub blob_contents: HashMap<ObjectId, Vec<u8>>,
    // Map from blob ID to its parent directory so we can implement `..` in
    // path traversal. We add to this every time we open a file.
    // There's no garbage collection currently - if you open a directory
    // and close it this will stay here. This would be relatively easy to fix
    // with a reference count, but it's probably not worth it in this case.
    pub parent: HashMap<ObjectId, ObjectId>,
}

impl GitFs {
    // Follow a path relative to an existing file or directory.
    // See https://pubs.opengroup.org/onlinepubs/9799919799/ for details about
    // POSIX's mad pathname resolution, and https://github.com/WebAssembly/wasi-filesystem/blob/main/path-resolution.md
    // for WASI specifically.
    //
    // Only relative paths are allowed. Absolute paths cause a permission error.
    // For this function the target file or directory (or symlink) must exist.
    fn resolve_path(&mut self, from: Descriptor, relative_path: &str, follow_final_symlink: bool) -> FsResult<Descriptor> {
        if relative_path.starts_with('/') {
            return Err(ErrorCode::Access.into());
        }

        let mut descriptor = from;

        // TODO: Allow a maximum of 40 symlink follows. Based on this value
        // https://github.com/wasix-org/wasix-libc/blob/28158c2ece7401604a9f6a409be320b47fffe78e/expected/wasm32-wasi/predefined-macros.txt#L4617
        let mut symlink_follow_remaining = 40;

        // So we can handle the last component separately.
        for component in relative_path.split('/') {
            match descriptor.kind {
                EntryKind::Tree => {
                    match component {
                        // Either two consecutive slashes "foo/bar//baz" or a trailing slash "foo/bar/".
                        "" => continue,
                        "." => continue,
                        ".." => {
                            // If there's no parent we're trying to .. above the root, which is not allowed by WASI.
                            descriptor.id = *self.parent.get(&descriptor.id).ok_or(ErrorCode::Access)?;
                            // Parent directory must be a directory.
                            descriptor.kind = EntryKind::Tree;
                        }
                        // Named child.
                        _ => {
                            // Open the current directory and find the child component.
                            let tree = self.repo.find_tree(descriptor.id).map_err(|_| ErrorCode::NoEntry)?;
                            // Find the child object.
                            let entry = tree.find_entry(component).ok_or(ErrorCode::NoEntry)?;

                            descriptor.id = entry.id().detach();
                            descriptor.kind = entry.kind();
                        }
                    }
                }
                EntryKind::Blob | EntryKind::BlobExecutable => {
                    // Can't get a child of a file.
                    return Err(ErrorCode::NotDirectory.into());
                }
                EntryKind::Link => {
                    todo!("symlink support")
                },
                EntryKind::Commit => todo!(),
            }
        }

        if descriptor.kind == EntryKind::Link && follow_final_symlink {
            todo!("symlink support")
        }
        Ok(descriptor)
    }
}

fn gix_entry_kind_to_descriptor_type(kind: EntryKind) -> DescriptorType {
    match kind {
        EntryKind::Tree => DescriptorType::Directory,
        EntryKind::Blob | EntryKind::BlobExecutable => DescriptorType::RegularFile,
        EntryKind::Link => DescriptorType::SymbolicLink,
        // For simplicity, submodules are treated as empty directories.
        EntryKind::Commit => DescriptorType::Directory,
    }
}

// The preopens are the only place the filesystem is provided a Descriptor,
// from which to try open_at to get more Descriptors. If we don't provide
// anything here, none of the methods on Descriptor will ever be reachable,
// because Resources are unforgable (the runtime will trap bogus indexes).
impl wasi_fs::wasi::filesystem::preopens::Host for WasiState {
    fn get_directories(
        &mut self,
    ) -> anyhow::Result<
        Vec<(
            Resource<Descriptor>,
            String,
        )>,
    > {
        // We have one hard-coded pre-open: `/`.
        Ok(vec![(
            // Create a new file descriptor and add it to the resource table,
            // returning its index in the table.
            self.resource_table.push(Descriptor{
                kind: EntryKind::Tree,
                id: self.gitfs.root,
            }).with_context(|| format!("failed to push root preopen"))?,
            // Path
            "/".to_string(),
        )])
    }
}

// Allow performing all the usual filesystem operations on a file descriptor.
impl wasi_fs::wasi::filesystem::types::HostDescriptor for WasiState {
    fn read_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
        offset: u64,
    ) -> FsResult<Resource<Box<(dyn wasmtime_wasi::p2::InputStream + 'static)>>> {
        todo!()
    }

    fn write_via_stream(
        &mut self,
        _fd: Resource<Descriptor>,
        _offset: u64,
    ) -> FsResult<Resource<Box<(dyn wasmtime_wasi::p2::OutputStream + 'static)>>> {
        Err(ErrorCode::ReadOnly.into())
    }

    fn append_via_stream(
        &mut self,
        _fd: Resource<Descriptor>,
    ) -> FsResult<Resource<Box<(dyn wasmtime_wasi::p2::OutputStream + 'static)>>> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn advise(
        &mut self,
        _fd: Resource<Descriptor>,
        _offset: Filesize,
        _length: Filesize,
        _advice: Advice,
    ) -> FsResult<()> {
        // Not used.
        Ok(())
    }

    async fn sync_data(&mut self, _fd: Resource<Descriptor>) -> FsResult<()> {
        //  Sync not needed.
        Ok(())
    }

    async fn get_flags(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorFlags> {
        // TODO: I guess we will need to record in the descriptor how it was opened.
        Ok(DescriptorFlags::READ)
    }

    async fn get_type(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorType> {
        let descriptor = self.resource_table.get(&fd).unwrap();
        Ok(gix_entry_kind_to_descriptor_type(descriptor.kind))
    }

    async fn set_size(&mut self, _fd: Resource<Descriptor>, _size: Filesize) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn set_times(
        &mut self,
        _fd: Resource<Descriptor>,
        _data_access_timestamp: NewTimestamp,
        _data_modification_timestamp: NewTimestamp,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn read(
        &mut self,
        fd: Resource<Descriptor>,
        length: Filesize,
        offset: Filesize,
    ) -> FsResult<(Vec<u8>, bool)> {
        let descriptor = self.resource_table.get_mut(&fd).unwrap();
        todo!()
    }

    async fn write(
        &mut self,
        _fd: Resource<Descriptor>,
        _buffer: Vec<u8>,
        _offset: Filesize,
    ) -> FsResult<Filesize> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn read_directory(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> FsResult<Resource<ReaddirIterator>> {
        let descriptor = self.resource_table.get(&fd).unwrap();
        // TODO: Could use `find_tree_iter()` ideally but I don't know if the
        // lifetime issues are easy to deal with, or if it makes any performance difference.
        let tree = self.gitfs.repo.find_tree(descriptor.id).unwrap();
        let mut entries: Vec<_> = tree.iter().map(|entry| {
            let entry = entry.unwrap();
            DirectoryEntry {
                type_: gix_entry_kind_to_descriptor_type(entry.kind()),
                name: entry.filename().to_string(),
            }
        }).collect();
        // Reverse because we pop them off the back when reading.
        // TODO: Probably can do this more efficiently somehow.
        entries.reverse();
        Ok(self.resource_table.push(ReaddirIterator{entries}).unwrap())
    }

    async fn sync(&mut self, _fd: Resource<Descriptor>) -> FsResult<()> {
        // Sync not needed.
        Ok(())
    }

    async fn create_directory_at(
        &mut self,
        _fd: Resource<Descriptor>,
        _path: String,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn stat(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorStat> {
        let descriptor = self.resource_table.get(&fd).unwrap();
        Ok(DescriptorStat {
            type_: gix_entry_kind_to_descriptor_type(descriptor.kind),
            // Git doesn't support hard links and the normal case is 1, not 0.
            link_count: 1,
            // In posix for symlinks this is the size of the path. Does that apply here?
            size: match descriptor.kind {
                // For symlinks this should return the size of the path, which Git
                // conveniently stores as the blob data, so we can use the same code.
                EntryKind::Blob | EntryKind::BlobExecutable | EntryKind::Link => self.gitfs.repo.find_header(descriptor.id).unwrap().size(),
                // Directory or submodule.
                EntryKind::Tree | EntryKind::Commit => 0,
            },
            // Git doesn't record this.
            data_access_timestamp: None,
            data_modification_timestamp: None,
            status_change_timestamp: None,
        })
    }

    async fn stat_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<DescriptorStat> {
        let from_descriptor = self.resource_table.get(&fd).unwrap();
        let follow_final_symlink: bool = path_flags.contains(PathFlags::SYMLINK_FOLLOW);
        let descriptor = self.gitfs.resolve_path(*from_descriptor, &path, follow_final_symlink)?;

        // TODO: Extract into function.
        Ok(DescriptorStat {
            type_: gix_entry_kind_to_descriptor_type(descriptor.kind),
            // Git doesn't support hard links and the normal case is 1, not 0.
            link_count: 1,
            // In posix for symlinks this is the size of the path. Does that apply here?
            size: match descriptor.kind {
                // For symlinks this should return the size of the path, which Git
                // conveniently stores as the blob data, so we can use the same code.
                EntryKind::Blob | EntryKind::BlobExecutable | EntryKind::Link => self.gitfs.repo.find_header(descriptor.id).unwrap().size(),
                // Directory or submodule.
                EntryKind::Tree | EntryKind::Commit => 0,
            },
            // Git doesn't record this.
            data_access_timestamp: None,
            data_modification_timestamp: None,
            status_change_timestamp: None,
        })
    }

    async fn set_times_at(
        &mut self,
        _fd: Resource<Descriptor>,
        _path_flags: PathFlags,
        _path: String,
        _data_access_timestamp: NewTimestamp,
        _data_modification_timestamp: NewTimestamp,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn link_at(
        &mut self,
        _fd: Resource<Descriptor>,
        _old_path_flags: PathFlags,
        _old_path: String,
        _new_descriptor: Resource<Descriptor>,
        _new_path: String,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    // Open the relative path `path`, relative to the directory `fd`. Unlike
    // POSIX `openat` path must be relative.
    async fn open_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
        open_flags: OpenFlags,
        flags: DescriptorFlags,
    ) -> FsResult<Resource<Descriptor>> {
        if open_flags.contains(OpenFlags::CREATE) || open_flags.contains(OpenFlags::TRUNCATE) || flags.contains(DescriptorFlags::WRITE) {
            return Err(ErrorCode::ReadOnly.into());
        }

        // TODO: Handle other DescriptorFlags maybe.

        let from_descriptor = self.resource_table.get(&fd).unwrap();
        let follow_final_symlink: bool = path_flags.contains(PathFlags::SYMLINK_FOLLOW);
        let descriptor = self.gitfs.resolve_path(*from_descriptor, &path, follow_final_symlink)?;

        if open_flags.contains(OpenFlags::EXCLUSIVE) {
            return Err(ErrorCode::Exist.into());
        }

        if open_flags.contains(OpenFlags::DIRECTORY) && descriptor.kind != EntryKind::Tree {
            return Err(ErrorCode::NotDirectory.into());
        }

        Ok(self.resource_table.push(descriptor).unwrap())
    }

    async fn readlink_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<String> {
        let from_descriptor = self.resource_table.get(&fd).unwrap();
        let descriptor = self.gitfs.resolve_path(*from_descriptor, &path, false)?;

        if descriptor.kind != EntryKind::Link {
            return Err(ErrorCode::Invalid.into());
        }

        let mut link = self.gitfs.repo.find_blob(descriptor.id).map_err(|_| ErrorCode::NoEntry)?;
        let link_str = String::from_utf8(std::mem::take(&mut link.data)).map_err(|_| ErrorCode::IllegalByteSequence)?;
        Ok(link_str.to_owned())
    }

    async fn remove_directory_at(
        &mut self,
        _fd: Resource<Descriptor>,
        _path: String,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn rename_at(
        &mut self,
        _fd: Resource<Descriptor>,
        _old_path: String,
        _new_descriptor: Resource<Descriptor>,
        _new_path: String,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn symlink_at(
        &mut self,
        _fd: Resource<Descriptor>,
        _old_path: String,
        _new_path: String,
    ) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn unlink_file_at(&mut self, _fd: Resource<Descriptor>, _path: String) -> FsResult<()> {
        Err(ErrorCode::ReadOnly.into())
    }

    async fn is_same_object(
        &mut self,
        fd: Resource<Descriptor>,
        other: Resource<Descriptor>,
    ) -> wasmtime::Result<bool> {
        let fd = self.resource_table.get(&fd).unwrap();
        let other = self.resource_table.get(&other).unwrap();
        Ok(fd == other)
    }

    async fn metadata_hash(&mut self, fd: Resource<Descriptor>) -> FsResult<MetadataHashValue> {
        // Kind of unclear what the use case for this is if you ask me.
        // While this is read-only we can just return the object ID which is long enough.
        let descriptor = self.resource_table.get(&fd).unwrap();
        Ok(MetadataHashValue{
            lower: u64::from_le_bytes(descriptor.id.as_bytes()[0..8].try_into().unwrap()),
            upper: u64::from_le_bytes(descriptor.id.as_bytes()[8..16].try_into().unwrap()),
        })
    }

    async fn metadata_hash_at(
        &mut self,
        fd: Resource<Descriptor>,
        _path_flags: PathFlags,
        _path: String,
    ) -> FsResult<MetadataHashValue> {
        // Kind of unclear what the use case for this is if you ask me.
        // While this is read-only we can just return the object ID which is long enough.
        let descriptor = self.resource_table.get(&fd).unwrap();
        Ok(MetadataHashValue{
            lower: u64::from_le_bytes(descriptor.id.as_bytes()[0..8].try_into().unwrap()),
            upper: u64::from_le_bytes(descriptor.id.as_bytes()[8..16].try_into().unwrap()),
        })
    }

    fn drop(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> anyhow::Result<()> {
        // This will drop the `Descriptor` which should close the file.
        self.resource_table.delete(fd)?;
        Ok(())
    }
}

// Allow iterating through a directory returned by `read_directory()`.
impl wasi_fs::wasi::filesystem::types::HostDirectoryEntryStream for WasiState {
    // Get the next directory entry or None.
    async fn read_directory_entry(
        &mut self,
        stream: Resource<ReaddirIterator>,
    ) -> FsResult<Option<DirectoryEntry>> {
        let stream = self.resource_table.get_mut(&stream).unwrap();
        Ok(stream.entries.pop())
    }

    fn drop(
        &mut self,
        stream: Resource<wasi_fs::wasi::filesystem::types::DirectoryEntryStream>,
    ) -> anyhow::Result<()> {
        self.resource_table.delete(stream)?;
        Ok(())
    }
}

impl wasi_fs::wasi::filesystem::types::Host for WasiState {
    fn convert_error_code(&mut self, err: FsError) -> wasmtime::Result<ErrorCode> {
        err.downcast()
    }

    fn filesystem_error_code(
        &mut self,
        err: Resource<anyhow::Error>,
    ) -> anyhow::Result<Option<ErrorCode>> {
        let err = self.resource_table.get(&err)?;

        // TODO: Do something here?

        Ok(None)
    }
}

// wasmtime has a super complicated nested layer of newtypes and traits to get
// around the orphan rule:
//
//   IoView: trait with .table() method returning ResourceTable.
//   WasiView: trait with .ctx() method returning WasiCtx. Inherits IoView
//   HasData: trait with ::Data associated type.
//
//   IoImpl<T>: Wrapper around T implementing IoView.
//   WasiImpl<T>: Wrapper around IoImpl<T> implementing IoView and WasiView.
//   HasWasi<T>: Wrapper around T providing HasData trait with ::Data set to WasiImpl<T>
//
// I could make this generic by adding a *third* layer of newtypes (`WasiFsImpl<T>`)
// but that just gets really complicated and this isn't a library so I'm using
// a concrete type `WasiState` instead.

struct HasWasiFs;

impl HasData for HasWasiFs {
    type Data<'a> = &'a mut WasiState;
}

pub fn add_to_linker_async(linker: &mut Linker<WasiState>) -> anyhow::Result<()> {
    wasi_fs::wasi::filesystem::types::add_to_linker::<WasiState, HasWasiFs>(linker, |t| t)?;
    wasi_fs::wasi::filesystem::preopens::add_to_linker::<WasiState, HasWasiFs>(linker, |t| t)?;
    Ok(())
}
