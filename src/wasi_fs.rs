use crate::wasi_fs::wasi::filesystem::types::{DirectoryEntry, ErrorCode};
use gix::{objs::tree::EntryKind, ObjectId};
use wasmtime_wasi::TrappableError;

pub type FsResult<T> = Result<T, FsError>;
pub type FsError = TrappableError<ErrorCode>;

// A descriptor is the state associated with a file descriptor. It is stored
// in the resource table. Normally this would hold any information you need
// to access the underlying file/directory (e.g. a POSIX file descriptor).
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Descriptor {
    // What kind of Git object it is (blob, tree etc.)
    pub kind: EntryKind,
    // Git commit ID.
    pub id: ObjectId,
}

// Type returned by `read_dir()` that allows iterating through directory entries.
pub struct ReaddirIterator {
    pub entries: Vec<DirectoryEntry>,
}
