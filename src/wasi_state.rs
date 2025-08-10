use crate::wasi_fs::{
    self, wasi::filesystem::types::{
        Advice, DescriptorFlags, DescriptorStat, DescriptorType, DirectoryEntry, ErrorCode,
        Filesize, MetadataHashValue, NewTimestamp, OpenFlags, PathFlags,
    }, Descriptor, FsError, FsResult, ReaddirIterator
};
use anyhow::Context as _;
use gix::{objs::FindExt, ObjectId, Repository};
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
    // Git repository.
    pub repo: Repository,
    // Root tree object ID.
    pub root: ObjectId,
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
            self.resource_table.push(Descriptor::Tree(self.root)).with_context(|| format!("failed to push root preopen"))?,
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
        fd: Resource<Descriptor>,
        offset: u64,
    ) -> FsResult<Resource<Box<(dyn wasmtime_wasi::p2::OutputStream + 'static)>>> {
        todo!()
    }

    fn append_via_stream(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> FsResult<Resource<Box<(dyn wasmtime_wasi::p2::OutputStream + 'static)>>> {
        todo!()
    }

    async fn advise(
        &mut self,
        fd: Resource<Descriptor>,
        offset: Filesize,
        length: Filesize,
        advice: Advice,
    ) -> FsResult<()> {
        todo!()
    }

    async fn sync_data(&mut self, fd: Resource<Descriptor>) -> FsResult<()> {
        todo!()
    }

    async fn get_flags(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorFlags> {
        todo!()
    }

    async fn get_type(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorType> {
        let descriptor = self.resource_table.get(&fd).unwrap();
        Ok(match descriptor {
            Descriptor::Blob(_) => DescriptorType::RegularFile,
            Descriptor::Tree(_) => DescriptorType::Directory,
            // TODO: Symlink.
        })
    }

    async fn set_size(&mut self, fd: Resource<Descriptor>, size: Filesize) -> FsResult<()> {
        todo!()
    }

    async fn set_times(
        &mut self,
        fd: Resource<Descriptor>,
        data_access_timestamp: NewTimestamp,
        data_modification_timestamp: NewTimestamp,
    ) -> FsResult<()> {
        todo!()
    }

    async fn read(
        &mut self,
        fd: Resource<Descriptor>,
        length: Filesize,
        offset: Filesize,
    ) -> FsResult<(Vec<u8>, bool)> {
        todo!()
    }

    async fn write(
        &mut self,
        fd: Resource<Descriptor>,
        buffer: Vec<u8>,
        offset: Filesize,
    ) -> FsResult<Filesize> {
        todo!()
    }

    async fn read_directory(
        &mut self,
        fd: Resource<Descriptor>,
    ) -> FsResult<Resource<ReaddirIterator>> {
        let descriptor = self.resource_table.get(&fd).unwrap();
        match descriptor {
            Descriptor::Blob(object_id) => todo!(),
            Descriptor::Tree(object_id) => {
                // TODO: Could use `find_tree_iter()` ideally but I don't know if the
                // lifetime issues are easy to deal with, or if it makes any performance difference.
                let tree = self.repo.find_tree(*object_id).unwrap();
                let entries: Vec<String> = tree.iter().map(|entry| entry.unwrap().filename().to_string()).collect();
                Ok(self.resource_table.push(ReaddirIterator{entries}).unwrap())
            }
        }
    }

    async fn sync(&mut self, fd: Resource<Descriptor>) -> FsResult<()> {
        todo!()
    }

    async fn create_directory_at(
        &mut self,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        todo!()
    }

    async fn stat(&mut self, fd: Resource<Descriptor>) -> FsResult<DescriptorStat> {
        todo!()
    }

    async fn stat_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<DescriptorStat> {
        todo!()
    }

    async fn set_times_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
        data_access_timestamp: NewTimestamp,
        data_modification_timestamp: NewTimestamp,
    ) -> FsResult<()> {
        todo!()
    }

    async fn link_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path_flags: PathFlags,
        old_path: String,
        new_descriptor: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        todo!()
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
        if path == "." {
            // Make a copy of the `fd` descriptor.
            let descriptor = self.resource_table.get(&fd).unwrap();
            Ok(self.resource_table.push(*descriptor).unwrap())
        } else {
            todo!();
        }
    }

    async fn readlink_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<String> {
        todo!()
    }

    async fn remove_directory_at(
        &mut self,
        fd: Resource<Descriptor>,
        path: String,
    ) -> FsResult<()> {
        todo!()
    }

    async fn rename_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path: String,
        new_descriptor: Resource<Descriptor>,
        new_path: String,
    ) -> FsResult<()> {
        todo!()
    }

    async fn symlink_at(
        &mut self,
        fd: Resource<Descriptor>,
        old_path: String,
        new_path: String,
    ) -> FsResult<()> {
        todo!()
    }

    async fn unlink_file_at(&mut self, fd: Resource<Descriptor>, path: String) -> FsResult<()> {
        todo!()
    }

    async fn is_same_object(
        &mut self,
        fd: Resource<Descriptor>,
        other: Resource<Descriptor>,
    ) -> wasmtime::Result<bool> {
        todo!()
    }

    async fn metadata_hash(&mut self, fd: Resource<Descriptor>) -> FsResult<MetadataHashValue> {
        // Kind of unclear what the use case for this is if you ask me.
        Ok(MetadataHashValue{lower: 0, upper: 0})
    }

    async fn metadata_hash_at(
        &mut self,
        fd: Resource<Descriptor>,
        path_flags: PathFlags,
        path: String,
    ) -> FsResult<MetadataHashValue> {
        // Kind of unclear what the use case for this is if you ask me.
        Ok(MetadataHashValue{lower: 0, upper: 0})
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
        let entry = stream.entries.pop();
        Ok(entry.map(|name| DirectoryEntry { type_: DescriptorType::RegularFile, name }))
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
    fn filesystem_error_code(
        &mut self,
        _: Resource<wasmtime_wasi_io::streams::Error>,
    ) -> anyhow::Result<Option<wasi_fs::wasi::filesystem::types::ErrorCode>> {
        todo!()
    }

    fn convert_error_code(&mut self, err: FsError) -> wasmtime::Result<ErrorCode> {
        todo!()
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
