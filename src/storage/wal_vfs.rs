//! A SQLite VFS wrapper that imposes a hard logical byte limit on WAL files.
//!
//! The normal database page limit does not cap a WAL file while readers prevent
//! checkpoints. This VFS delegates to SQLite's platform VFS and checks every WAL
//! write/truncate before it reaches the filesystem.

use rusqlite::{ffi, Connection, Error, OpenFlags, Result};
use std::{
    ffi::{c_char, c_int, c_void},
    mem::{align_of, size_of},
    path::Path,
    ptr,
    sync::OnceLock,
};

/// Maximum logical size of a WAL file, in bytes.
pub const LIMIT: u64 = 16_000_000;

const VFS_NAME: &[u8] = b"cogfabric_wal_limit_16000000_v1\0";
const FCNTL_LIMIT_MARKER: c_int = 0x4346_4701;

/// The connection buffer starts with this structure and contains the platform
/// VFS's `sqlite3_file` immediately after it, at an aligned offset.
#[repr(C)]
struct WrappedFile {
    base: ffi::sqlite3_file,
    methods: ffi::sqlite3_io_methods,
    real: *mut ffi::sqlite3_file,
    wrapper_vfs: *mut ffi::sqlite3_vfs,
    is_wal: bool,
}

// SQLite may call these callbacks from different threads. The registered VFS
// is immutable after sqlite3_vfs_register except for SQLite's own pNext link.
// It and the default platform VFS both have process lifetime.
static REGISTERED: OnceLock<std::result::Result<usize, c_int>> = OnceLock::new();

fn error(code: c_int) -> Error {
    Error::SqliteFailure(ffi::Error::new(code), None)
}

fn wrapper_vfs() -> Result<*mut ffi::sqlite3_vfs> {
    let result = REGISTERED.get_or_init(|| unsafe {
        let parent = ffi::sqlite3_vfs_find(ptr::null());
        if parent.is_null() {
            return Err(ffi::SQLITE_ERROR);
        }
        let parent_file_size = (*parent).szOsFile;
        if parent_file_size <= 0 {
            return Err(ffi::SQLITE_ERROR);
        }
        let Some(file_offset) = aligned_file_offset() else {
            return Err(ffi::SQLITE_TOOBIG);
        };
        let Some(total_file_size) = file_offset.checked_add(parent_file_size as usize) else {
            return Err(ffi::SQLITE_TOOBIG);
        };
        let Ok(sz_os_file) = c_int::try_from(total_file_size) else {
            return Err(ffi::SQLITE_TOOBIG);
        };

        let vfs = Box::new(make_vfs(parent, sz_os_file));
        let pointer = Box::into_raw(vfs);
        let rc = ffi::sqlite3_vfs_register(pointer, 0);
        if rc != ffi::SQLITE_OK {
            // Registration failed, so SQLite did not retain this pointer.
            drop(Box::from_raw(pointer));
            return Err(rc);
        }
        // SQLite stores this VFS pointer for the life of the process. The Box
        // is intentionally leaked after successful registration.
        Ok(pointer as usize)
    });
    result
        .map(|pointer| pointer as *mut ffi::sqlite3_vfs)
        .map_err(error)
}

fn aligned_file_offset() -> Option<usize> {
    let align = align_of::<ffi::sqlite3_file>();
    size_of::<WrappedFile>()
        .checked_add(align.checked_sub(1)?)
        .map(|size| size / align * align)
}

/// Open a SQLite database using the process-registered bounded WAL VFS.
pub(crate) fn open(path: &Path, flags: OpenFlags) -> Result<Connection> {
    wrapper_vfs()?;
    Connection::open_with_flags_and_vfs(path, flags, "cogfabric_wal_limit_16000000_v1")
}

/// Return the hard WAL bound only when `db` is actually using this wrapper.
pub(crate) fn enforced_limit(db: &Connection) -> Option<u64> {
    let mut limit = 0_u64;
    // SAFETY: The connection is live and the file-control opcode is handled by
    // our VFS. `limit` is a writable u64 output for the duration of the call.
    let rc = unsafe {
        ffi::sqlite3_file_control(
            db.handle(),
            c"main".as_ptr(),
            FCNTL_LIMIT_MARKER,
            (&mut limit as *mut u64).cast(),
        )
    };
    (rc == ffi::SQLITE_OK && limit == LIMIT).then_some(limit)
}

unsafe fn parent_vfs(vfs: *mut ffi::sqlite3_vfs) -> *mut ffi::sqlite3_vfs {
    // SAFETY: Every registered wrapper stores the process-lifetime parent VFS
    // pointer in pAppData before registration.
    (*vfs).pAppData.cast()
}

unsafe fn make_vfs(parent: *mut ffi::sqlite3_vfs, sz_os_file: c_int) -> ffi::sqlite3_vfs {
    let source = &*parent;
    ffi::sqlite3_vfs {
        iVersion: source.iVersion,
        szOsFile: sz_os_file,
        mxPathname: source.mxPathname,
        pNext: ptr::null_mut(),
        zName: VFS_NAME.as_ptr().cast(),
        pAppData: parent.cast(),
        xOpen: Some(vfs_open),
        xDelete: source.xDelete.map(|_| vfs_delete as _),
        xAccess: source.xAccess.map(|_| vfs_access as _),
        xFullPathname: source.xFullPathname.map(|_| vfs_full_pathname as _),
        xDlOpen: source.xDlOpen.map(|_| vfs_dl_open as _),
        xDlError: source.xDlError.map(|_| vfs_dl_error as _),
        xDlSym: source.xDlSym.map(|_| vfs_dl_sym as _),
        xDlClose: source.xDlClose.map(|_| vfs_dl_close as _),
        xRandomness: source.xRandomness.map(|_| vfs_randomness as _),
        xSleep: source.xSleep.map(|_| vfs_sleep as _),
        xCurrentTime: source.xCurrentTime.map(|_| vfs_current_time as _),
        xGetLastError: source.xGetLastError.map(|_| vfs_get_last_error as _),
        xCurrentTimeInt64: if source.iVersion >= 2 {
            source
                .xCurrentTimeInt64
                .map(|_| vfs_current_time_int64 as _)
        } else {
            None
        },
        xSetSystemCall: if source.iVersion >= 3 {
            source.xSetSystemCall.map(|_| vfs_set_system_call as _)
        } else {
            None
        },
        xGetSystemCall: if source.iVersion >= 3 {
            source.xGetSystemCall.map(|_| vfs_get_system_call as _)
        } else {
            None
        },
        xNextSystemCall: if source.iVersion >= 3 {
            source.xNextSystemCall.map(|_| vfs_next_system_call as _)
        } else {
            None
        },
    }
}

unsafe extern "C" fn vfs_open(
    vfs: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    if vfs.is_null() || file.is_null() {
        return ffi::SQLITE_MISUSE;
    }
    let parent = parent_vfs(vfs);
    if parent.is_null() {
        return ffi::SQLITE_ERROR;
    }
    let Some(open) = (*parent).xOpen else {
        return ffi::SQLITE_CANTOPEN;
    };
    let Some(offset) = aligned_file_offset() else {
        return ffi::SQLITE_TOOBIG;
    };
    let real = (file.cast::<u8>()).add(offset).cast::<ffi::sqlite3_file>();
    ptr::write(
        file.cast::<WrappedFile>(),
        WrappedFile {
            base: ffi::sqlite3_file {
                pMethods: ptr::null(),
            },
            methods: empty_methods(),
            real,
            wrapper_vfs: vfs,
            is_wal: flags & ffi::SQLITE_OPEN_WAL != 0,
        },
    );

    let rc = open(parent, name, real, flags, out_flags);
    let wrapped = &mut *file.cast::<WrappedFile>();
    if rc != ffi::SQLITE_OK {
        if !(*real).pMethods.is_null() {
            if let Some(close) = (*(*real).pMethods).xClose {
                let _ = close(real);
            }
        }
        wrapped.base.pMethods = ptr::null();
        return rc;
    }
    if (*real).pMethods.is_null() {
        return ffi::SQLITE_IOERR;
    }

    wrapped.methods = wrap_methods((*real).pMethods);
    wrapped.base.pMethods = &wrapped.methods;

    // An existing oversized WAL remains readable so SQLite can inspect and
    // checkpoint it. `file_write` refuses further writes while it is oversized,
    // while a truncate to a size at or below LIMIT remains available for
    // checkpoint recovery. This VFS never silently edits existing bytes.
    ffi::SQLITE_OK
}

fn empty_methods() -> ffi::sqlite3_io_methods {
    ffi::sqlite3_io_methods {
        iVersion: 0,
        xClose: None,
        xRead: None,
        xWrite: None,
        xTruncate: None,
        xSync: None,
        xFileSize: None,
        xLock: None,
        xUnlock: None,
        xCheckReservedLock: None,
        xFileControl: None,
        xSectorSize: None,
        xDeviceCharacteristics: None,
        xShmMap: None,
        xShmLock: None,
        xShmBarrier: None,
        xShmUnmap: None,
        xFetch: None,
        xUnfetch: None,
    }
}

unsafe fn wrap_methods(source: *const ffi::sqlite3_io_methods) -> ffi::sqlite3_io_methods {
    let version = (*source).iVersion;
    ffi::sqlite3_io_methods {
        iVersion: version,
        xClose: (*source).xClose.map(|_| file_close as _),
        xRead: (*source).xRead.map(|_| file_read as _),
        xWrite: (*source).xWrite.map(|_| file_write as _),
        xTruncate: (*source).xTruncate.map(|_| file_truncate as _),
        xSync: (*source).xSync.map(|_| file_sync as _),
        xFileSize: (*source).xFileSize.map(|_| file_size as _),
        xLock: (*source).xLock.map(|_| file_lock as _),
        xUnlock: (*source).xUnlock.map(|_| file_unlock as _),
        xCheckReservedLock: (*source)
            .xCheckReservedLock
            .map(|_| file_check_reserved_lock as _),
        // xFileControl also serves the private VFS-marker query.
        xFileControl: Some(file_control),
        xSectorSize: (*source).xSectorSize.map(|_| file_sector_size as _),
        xDeviceCharacteristics: (*source)
            .xDeviceCharacteristics
            .map(|_| file_device_characteristics as _),
        xShmMap: if version >= 2 {
            (*source).xShmMap.map(|_| file_shm_map as _)
        } else {
            None
        },
        xShmLock: if version >= 2 {
            (*source).xShmLock.map(|_| file_shm_lock as _)
        } else {
            None
        },
        xShmBarrier: if version >= 2 && (*source).xShmBarrier.is_some() {
            Some(file_shm_barrier as _)
        } else {
            None
        },
        xShmUnmap: if version >= 2 {
            (*source).xShmUnmap.map(|_| file_shm_unmap as _)
        } else {
            None
        },
        xFetch: if version >= 3 {
            (*source).xFetch.map(|_| file_fetch as _)
        } else {
            None
        },
        xUnfetch: if version >= 3 {
            (*source).xUnfetch.map(|_| file_unfetch as _)
        } else {
            None
        },
    }
}

unsafe fn wrapped(file: *mut ffi::sqlite3_file) -> *mut WrappedFile {
    file.cast()
}

unsafe fn real_file(file: *mut ffi::sqlite3_file) -> *mut ffi::sqlite3_file {
    (*wrapped(file)).real
}

unsafe fn real_methods(file: *mut ffi::sqlite3_file) -> *const ffi::sqlite3_io_methods {
    (*real_file(file)).pMethods
}

unsafe extern "C" fn close_wrapped(file: *mut ffi::sqlite3_file) -> c_int {
    if file.is_null() {
        return ffi::SQLITE_MISUSE;
    }
    let real = real_file(file);
    let rc = if real.is_null() || (*real).pMethods.is_null() {
        ffi::SQLITE_OK
    } else if let Some(close) = (*(*real).pMethods).xClose {
        close(real)
    } else {
        ffi::SQLITE_IOERR_CLOSE
    };
    (*file).pMethods = ptr::null();
    rc
}

unsafe extern "C" fn file_close(file: *mut ffi::sqlite3_file) -> c_int {
    close_wrapped(file)
}

unsafe extern "C" fn file_read(
    file: *mut ffi::sqlite3_file,
    buffer: *mut c_void,
    amount: c_int,
    offset: ffi::sqlite3_int64,
) -> c_int {
    match (*real_methods(file)).xRead {
        Some(callback) => callback(real_file(file), buffer, amount, offset),
        None => ffi::SQLITE_IOERR_READ,
    }
}

unsafe extern "C" fn file_write(
    file: *mut ffi::sqlite3_file,
    buffer: *const c_void,
    amount: c_int,
    offset: ffi::sqlite3_int64,
) -> c_int {
    let wrapped = &*wrapped(file);
    let real = wrapped.real;
    if wrapped.is_wal {
        if amount < 0 || offset < 0 {
            return ffi::SQLITE_IOERR_WRITE;
        }
        let Some(end) = (offset as u64).checked_add(amount as u64) else {
            return ffi::SQLITE_FULL;
        };
        if end > LIMIT {
            return ffi::SQLITE_FULL;
        }
        let Some(file_size) = (*(*real).pMethods).xFileSize else {
            return ffi::SQLITE_IOERR_FSTAT;
        };
        let mut current_size = 0_i64;
        let rc = file_size(real, &mut current_size);
        if rc != ffi::SQLITE_OK {
            return rc;
        }
        if current_size < 0 || current_size as u64 > LIMIT {
            return ffi::SQLITE_FULL;
        }
    }
    match (*(*real).pMethods).xWrite {
        Some(callback) => callback(real, buffer, amount, offset),
        None => ffi::SQLITE_IOERR_WRITE,
    }
}

unsafe extern "C" fn file_truncate(
    file: *mut ffi::sqlite3_file,
    size: ffi::sqlite3_int64,
) -> c_int {
    let wrapped = &*wrapped(file);
    if wrapped.is_wal && (size < 0 || size as u64 > LIMIT) {
        return ffi::SQLITE_FULL;
    }
    match (*(*wrapped.real).pMethods).xTruncate {
        Some(callback) => callback(wrapped.real, size),
        None => ffi::SQLITE_IOERR_TRUNCATE,
    }
}

unsafe extern "C" fn file_sync(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    match (*real_methods(file)).xSync {
        Some(callback) => callback(real_file(file), flags),
        None => ffi::SQLITE_IOERR_FSYNC,
    }
}

unsafe extern "C" fn file_size(
    file: *mut ffi::sqlite3_file,
    size: *mut ffi::sqlite3_int64,
) -> c_int {
    match (*real_methods(file)).xFileSize {
        Some(callback) => callback(real_file(file), size),
        None => ffi::SQLITE_IOERR_FSTAT,
    }
}

unsafe extern "C" fn file_lock(file: *mut ffi::sqlite3_file, lock: c_int) -> c_int {
    match (*real_methods(file)).xLock {
        Some(callback) => callback(real_file(file), lock),
        None => ffi::SQLITE_IOERR_LOCK,
    }
}

unsafe extern "C" fn file_unlock(file: *mut ffi::sqlite3_file, lock: c_int) -> c_int {
    match (*real_methods(file)).xUnlock {
        Some(callback) => callback(real_file(file), lock),
        None => ffi::SQLITE_IOERR_UNLOCK,
    }
}

unsafe extern "C" fn file_check_reserved_lock(
    file: *mut ffi::sqlite3_file,
    result: *mut c_int,
) -> c_int {
    match (*real_methods(file)).xCheckReservedLock {
        Some(callback) => callback(real_file(file), result),
        None => ffi::SQLITE_IOERR_CHECKRESERVEDLOCK,
    }
}

unsafe extern "C" fn file_control(
    file: *mut ffi::sqlite3_file,
    operation: c_int,
    argument: *mut c_void,
) -> c_int {
    let wrapped = &mut *wrapped(file);
    match operation {
        FCNTL_LIMIT_MARKER => {
            if argument.is_null() {
                return ffi::SQLITE_MISUSE;
            }
            *argument.cast::<u64>() = LIMIT;
            return ffi::SQLITE_OK;
        }
        ffi::SQLITE_FCNTL_FILE_POINTER => {
            if argument.is_null() {
                return ffi::SQLITE_MISUSE;
            }
            *argument.cast::<*mut ffi::sqlite3_file>() = &mut wrapped.base;
            return ffi::SQLITE_OK;
        }
        ffi::SQLITE_FCNTL_VFS_POINTER => {
            if argument.is_null() {
                return ffi::SQLITE_MISUSE;
            }
            *argument.cast::<*mut ffi::sqlite3_vfs>() = wrapped.wrapper_vfs;
            return ffi::SQLITE_OK;
        }
        ffi::SQLITE_FCNTL_SIZE_HINT | ffi::SQLITE_FCNTL_CHUNK_SIZE if wrapped.is_wal => {
            // The platform VFS may preallocate or round the file. The hard
            // bound is enforced at xWrite/xTruncate, so do not forward hints.
            return ffi::SQLITE_OK;
        }
        ffi::SQLITE_FCNTL_SIZE_LIMIT if wrapped.is_wal => {
            if argument.is_null() {
                return ffi::SQLITE_MISUSE;
            }
            let requested = &mut *argument.cast::<ffi::sqlite3_int64>();
            let Some(file_size) = (*(*wrapped.real).pMethods).xFileSize else {
                return ffi::SQLITE_IOERR_FSTAT;
            };
            let mut current_size = 0_i64;
            let rc = file_size(wrapped.real, &mut current_size);
            if rc != ffi::SQLITE_OK {
                return rc;
            }
            if current_size < 0 || current_size as u64 > LIMIT || *requested < 0 {
                *requested = LIMIT as i64;
            } else {
                *requested = (*requested).max(current_size).min(LIMIT as i64);
            }
            return ffi::SQLITE_OK;
        }
        _ if wrapped.is_wal && operation > 100 => {
            // Unknown extension controls must not receive the raw platform
            // file handle and circumvent the wrapped write path.
            return ffi::SQLITE_NOTFOUND;
        }
        _ => {}
    }
    match (*(*wrapped.real).pMethods).xFileControl {
        Some(callback) => callback(wrapped.real, operation, argument),
        None => ffi::SQLITE_NOTFOUND,
    }
}

unsafe extern "C" fn file_sector_size(file: *mut ffi::sqlite3_file) -> c_int {
    match (*real_methods(file)).xSectorSize {
        Some(callback) => callback(real_file(file)),
        None => 0,
    }
}

unsafe extern "C" fn file_device_characteristics(file: *mut ffi::sqlite3_file) -> c_int {
    match (*real_methods(file)).xDeviceCharacteristics {
        Some(callback) => callback(real_file(file)),
        None => 0,
    }
}

unsafe extern "C" fn file_shm_map(
    file: *mut ffi::sqlite3_file,
    page: c_int,
    page_size: c_int,
    extend: c_int,
    result: *mut *mut c_void,
) -> c_int {
    match (*real_methods(file)).xShmMap {
        Some(callback) => callback(real_file(file), page, page_size, extend, result),
        None => ffi::SQLITE_IOERR_SHMMAP,
    }
}

unsafe extern "C" fn file_shm_lock(
    file: *mut ffi::sqlite3_file,
    offset: c_int,
    count: c_int,
    flags: c_int,
) -> c_int {
    match (*real_methods(file)).xShmLock {
        Some(callback) => callback(real_file(file), offset, count, flags),
        None => ffi::SQLITE_IOERR_SHMLOCK,
    }
}

unsafe extern "C" fn file_shm_barrier(file: *mut ffi::sqlite3_file) {
    if let Some(callback) = (*real_methods(file)).xShmBarrier {
        callback(real_file(file));
    }
}

unsafe extern "C" fn file_shm_unmap(file: *mut ffi::sqlite3_file, delete: c_int) -> c_int {
    match (*real_methods(file)).xShmUnmap {
        Some(callback) => callback(real_file(file), delete),
        None => ffi::SQLITE_IOERR_SHMOPEN,
    }
}

unsafe extern "C" fn file_fetch(
    file: *mut ffi::sqlite3_file,
    offset: ffi::sqlite3_int64,
    amount: c_int,
    result: *mut *mut c_void,
) -> c_int {
    if (*wrapped(file)).is_wal {
        if result.is_null() {
            return ffi::SQLITE_MISUSE;
        }
        *result = ptr::null_mut();
        return ffi::SQLITE_OK;
    }
    match (*real_methods(file)).xFetch {
        Some(callback) => callback(real_file(file), offset, amount, result),
        None => {
            if !result.is_null() {
                *result = ptr::null_mut();
            }
            ffi::SQLITE_OK
        }
    }
}

unsafe extern "C" fn file_unfetch(
    file: *mut ffi::sqlite3_file,
    offset: ffi::sqlite3_int64,
    pointer: *mut c_void,
) -> c_int {
    if (*wrapped(file)).is_wal {
        return if pointer.is_null() {
            ffi::SQLITE_OK
        } else {
            ffi::SQLITE_IOERR
        };
    }
    match (*real_methods(file)).xUnfetch {
        Some(callback) => callback(real_file(file), offset, pointer),
        None => ffi::SQLITE_OK,
    }
}

unsafe extern "C" fn vfs_delete(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    sync_dir: c_int,
) -> c_int {
    match (*parent_vfs(vfs)).xDelete {
        Some(callback) => callback(parent_vfs(vfs), name, sync_dir),
        None => ffi::SQLITE_IOERR_DELETE,
    }
}

unsafe extern "C" fn vfs_access(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    flags: c_int,
    result: *mut c_int,
) -> c_int {
    match (*parent_vfs(vfs)).xAccess {
        Some(callback) => callback(parent_vfs(vfs), name, flags, result),
        None => ffi::SQLITE_IOERR_ACCESS,
    }
}

unsafe extern "C" fn vfs_full_pathname(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    output_size: c_int,
    output: *mut c_char,
) -> c_int {
    match (*parent_vfs(vfs)).xFullPathname {
        Some(callback) => callback(parent_vfs(vfs), name, output_size, output),
        None => ffi::SQLITE_CANTOPEN,
    }
}

unsafe extern "C" fn vfs_dl_open(
    vfs: *mut ffi::sqlite3_vfs,
    filename: *const c_char,
) -> *mut c_void {
    (*parent_vfs(vfs))
        .xDlOpen
        .map(|callback| callback(parent_vfs(vfs), filename))
        .unwrap_or(ptr::null_mut())
}

unsafe extern "C" fn vfs_dl_error(vfs: *mut ffi::sqlite3_vfs, size: c_int, message: *mut c_char) {
    if let Some(callback) = (*parent_vfs(vfs)).xDlError {
        callback(parent_vfs(vfs), size, message);
    }
}

unsafe extern "C" fn vfs_dl_sym(
    vfs: *mut ffi::sqlite3_vfs,
    handle: *mut c_void,
    symbol: *const c_char,
) -> Option<unsafe extern "C" fn(*mut ffi::sqlite3_vfs, *mut c_void, *const c_char)> {
    (*parent_vfs(vfs))
        .xDlSym
        .and_then(|callback| callback(parent_vfs(vfs), handle, symbol))
}

unsafe extern "C" fn vfs_dl_close(vfs: *mut ffi::sqlite3_vfs, handle: *mut c_void) {
    if let Some(callback) = (*parent_vfs(vfs)).xDlClose {
        callback(parent_vfs(vfs), handle);
    }
}

unsafe extern "C" fn vfs_randomness(
    vfs: *mut ffi::sqlite3_vfs,
    size: c_int,
    output: *mut c_char,
) -> c_int {
    (*parent_vfs(vfs))
        .xRandomness
        .map(|callback| callback(parent_vfs(vfs), size, output))
        .unwrap_or(0)
}

unsafe extern "C" fn vfs_sleep(vfs: *mut ffi::sqlite3_vfs, microseconds: c_int) -> c_int {
    (*parent_vfs(vfs))
        .xSleep
        .map(|callback| callback(parent_vfs(vfs), microseconds))
        .unwrap_or(0)
}

unsafe extern "C" fn vfs_current_time(vfs: *mut ffi::sqlite3_vfs, time: *mut f64) -> c_int {
    (*parent_vfs(vfs))
        .xCurrentTime
        .map(|callback| callback(parent_vfs(vfs), time))
        .unwrap_or(ffi::SQLITE_ERROR)
}

unsafe extern "C" fn vfs_get_last_error(
    vfs: *mut ffi::sqlite3_vfs,
    size: c_int,
    message: *mut c_char,
) -> c_int {
    (*parent_vfs(vfs))
        .xGetLastError
        .map(|callback| callback(parent_vfs(vfs), size, message))
        .unwrap_or(0)
}

unsafe extern "C" fn vfs_current_time_int64(
    vfs: *mut ffi::sqlite3_vfs,
    time: *mut ffi::sqlite3_int64,
) -> c_int {
    (*parent_vfs(vfs))
        .xCurrentTimeInt64
        .map(|callback| callback(parent_vfs(vfs), time))
        .unwrap_or(ffi::SQLITE_ERROR)
}

unsafe extern "C" fn vfs_set_system_call(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    callback: ffi::sqlite3_syscall_ptr,
) -> c_int {
    (*parent_vfs(vfs))
        .xSetSystemCall
        .map(|function| function(parent_vfs(vfs), name, callback))
        .unwrap_or(ffi::SQLITE_NOTFOUND)
}

unsafe extern "C" fn vfs_get_system_call(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
) -> ffi::sqlite3_syscall_ptr {
    (*parent_vfs(vfs))
        .xGetSystemCall
        .map(|callback| callback(parent_vfs(vfs), name))
        .unwrap_or(None)
}

unsafe extern "C" fn vfs_next_system_call(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
) -> *const c_char {
    (*parent_vfs(vfs))
        .xNextSystemCall
        .map(|callback| callback(parent_vfs(vfs), name))
        .unwrap_or(ptr::null())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    struct FakeState {
        size: AtomicI64,
        writes: AtomicUsize,
        truncates: AtomicUsize,
        controls: AtomicUsize,
    }

    #[repr(C)]
    struct FakeFile {
        base: ffi::sqlite3_file,
        state: *const FakeState,
    }

    unsafe extern "C" fn fake_close(_: *mut ffi::sqlite3_file) -> c_int {
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_read(
        _: *mut ffi::sqlite3_file,
        _: *mut c_void,
        _: c_int,
        _: ffi::sqlite3_int64,
    ) -> c_int {
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_write(
        file: *mut ffi::sqlite3_file,
        _: *const c_void,
        amount: c_int,
        offset: ffi::sqlite3_int64,
    ) -> c_int {
        let fake = &*file.cast::<FakeFile>();
        let end = offset.saturating_add(amount as i64);
        (*fake.state).size.fetch_max(end, Ordering::SeqCst);
        (*fake.state).writes.fetch_add(1, Ordering::SeqCst);
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_truncate(
        file: *mut ffi::sqlite3_file,
        size: ffi::sqlite3_int64,
    ) -> c_int {
        let fake = &*file.cast::<FakeFile>();
        (*fake.state).size.store(size, Ordering::SeqCst);
        (*fake.state).truncates.fetch_add(1, Ordering::SeqCst);
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_sync(_: *mut ffi::sqlite3_file, _: c_int) -> c_int {
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_size(
        file: *mut ffi::sqlite3_file,
        size: *mut ffi::sqlite3_int64,
    ) -> c_int {
        let fake = &*file.cast::<FakeFile>();
        *size = (*fake.state).size.load(Ordering::SeqCst);
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_lock(_: *mut ffi::sqlite3_file, _: c_int) -> c_int {
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_check_lock(_: *mut ffi::sqlite3_file, result: *mut c_int) -> c_int {
        *result = 0;
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_control(
        file: *mut ffi::sqlite3_file,
        _: c_int,
        _: *mut c_void,
    ) -> c_int {
        let fake = &*file.cast::<FakeFile>();
        (*fake.state).controls.fetch_add(1, Ordering::SeqCst);
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn fake_sector_size(_: *mut ffi::sqlite3_file) -> c_int {
        4096
    }

    unsafe extern "C" fn fake_device_characteristics(_: *mut ffi::sqlite3_file) -> c_int {
        0
    }

    const FAKE_METHODS: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
        iVersion: 1,
        xClose: Some(fake_close),
        xRead: Some(fake_read),
        xWrite: Some(fake_write),
        xTruncate: Some(fake_truncate),
        xSync: Some(fake_sync),
        xFileSize: Some(fake_size),
        xLock: Some(fake_lock),
        xUnlock: Some(fake_lock),
        xCheckReservedLock: Some(fake_check_lock),
        xFileControl: Some(fake_control),
        xSectorSize: Some(fake_sector_size),
        xDeviceCharacteristics: Some(fake_device_characteristics),
        xShmMap: None,
        xShmLock: None,
        xShmBarrier: None,
        xShmUnmap: None,
        xFetch: None,
        xUnfetch: None,
    };

    struct Fixture {
        state: Box<FakeState>,
        _real: Box<FakeFile>,
        wrapped: Box<WrappedFile>,
    }

    impl Fixture {
        fn new(size: i64) -> Self {
            let state = Box::new(FakeState {
                size: AtomicI64::new(size),
                writes: AtomicUsize::new(0),
                truncates: AtomicUsize::new(0),
                controls: AtomicUsize::new(0),
            });
            let real = Box::new(FakeFile {
                base: ffi::sqlite3_file {
                    pMethods: &FAKE_METHODS,
                },
                state: &*state,
            });
            let mut wrapped = Box::new(WrappedFile {
                base: ffi::sqlite3_file {
                    pMethods: ptr::null(),
                },
                methods: unsafe { wrap_methods(&FAKE_METHODS) },
                real: (&*real as *const FakeFile).cast_mut().cast(),
                wrapper_vfs: ptr::null_mut(),
                is_wal: true,
            });
            wrapped.base.pMethods = &wrapped.methods;
            Self {
                state,
                _real: real,
                wrapped,
            }
        }

        fn file(&mut self) -> *mut ffi::sqlite3_file {
            &mut self.wrapped.base
        }
    }

    #[test]
    fn write_and_truncate_refuse_negative_and_over_limit_sizes() {
        let mut fixture = Fixture::new(0);
        let data = 1_u8;
        // SAFETY: The fixture owns both file structures and keeps them alive
        // for each direct callback invocation.
        unsafe {
            assert_eq!(
                file_write(fixture.file(), (&data as *const u8).cast(), 1, -1),
                ffi::SQLITE_IOERR_WRITE
            );
            assert_eq!(
                file_write(fixture.file(), (&data as *const u8).cast(), 1, i64::MAX),
                ffi::SQLITE_FULL
            );
            assert_eq!(file_truncate(fixture.file(), -1), ffi::SQLITE_FULL);
            assert_eq!(
                file_truncate(fixture.file(), LIMIT as i64 + 1),
                ffi::SQLITE_FULL
            );
        }
        assert_eq!(fixture.state.writes.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.state.truncates.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn allowed_write_at_limit_and_truncate_can_recover_oversized_wal() {
        let mut fixture = Fixture::new(LIMIT as i64 + 4096);
        let data = 1_u8;
        // Oversized WAL writes fail, but a checkpoint can truncate it and the
        // final in-range write remains possible.
        unsafe {
            assert_eq!(
                file_write(
                    fixture.file(),
                    (&data as *const u8).cast(),
                    1,
                    LIMIT as i64 - 1
                ),
                ffi::SQLITE_FULL
            );
            assert_eq!(file_truncate(fixture.file(), 0), ffi::SQLITE_OK);
            assert_eq!(
                file_write(
                    fixture.file(),
                    (&data as *const u8).cast(),
                    1,
                    LIMIT as i64 - 1
                ),
                ffi::SQLITE_OK
            );
        }
        assert_eq!(fixture.state.size.load(Ordering::SeqCst), LIMIT as i64);
        assert_eq!(fixture.state.writes.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.state.truncates.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn wal_preallocation_hints_are_noops() {
        let mut fixture = Fixture::new(0);
        let mut requested_size = 1024_i64;
        let mut chunk_size = 4096_i32;
        unsafe {
            assert_eq!(
                file_control(
                    fixture.file(),
                    ffi::SQLITE_FCNTL_SIZE_HINT,
                    (&mut requested_size as *mut i64).cast()
                ),
                ffi::SQLITE_OK
            );
            assert_eq!(
                file_control(
                    fixture.file(),
                    ffi::SQLITE_FCNTL_CHUNK_SIZE,
                    (&mut chunk_size as *mut i32).cast()
                ),
                ffi::SQLITE_OK
            );
        }
        assert_eq!(fixture.state.controls.load(Ordering::SeqCst), 0);
    }
}
