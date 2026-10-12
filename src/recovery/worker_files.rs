//! Reviewed cleanup of journaled same-binary worker sockets under exclusive ownership.
use super::ExecutionLease;
use crate::{
    security,
    store::{sha256, Store},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SocketRecord {
    pub format_version: u16,
    pub name: String,
    pub uid: u32,
    pub directory_device: u64,
    pub directory_inode: u64,
    pub socket_device: u64,
    pub socket_inode: u64,
}
impl SocketRecord {
    fn validate(&self) -> Result<()> {
        let suffix = self
            .name
            .strip_prefix("cogfabric-tool-worker-")
            .or_else(|| self.name.strip_prefix("cogfabric-worker-"))
            .context("Unknown worker directory prefix")?;
        if self.format_version != 1
            || self.uid != unsafe { libc::geteuid() }
            || uuid::Uuid::parse_str(suffix)?.hyphenated().to_string() != suffix
        {
            bail!("Invalid worker socket identity")
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    event_id: u64,
    task_id: String,
    resource: SocketRecord,
    socket_present: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupPlan {
    workspace: (u64, u64),
    after: u64,
    limit: u32,
    pub cursor: u64,
    pub more: bool,
    pub entries: Vec<Entry>,
    checksum: String,
}
impl CleanupPlan {
    fn digest(&self) -> Result<String> {
        Ok(sha256(&serde_json::to_vec(&(
            self.workspace,
            self.after,
            self.limit,
            self.cursor,
            self.more,
            &self.entries,
        ))?))
    }
    fn validate(&self) -> Result<()> {
        if self.after > i64::MAX as u64
            || !(1..=128).contains(&self.limit)
            || self.entries.len() > self.limit as usize
            || self.checksum != self.digest()?
        {
            bail!("Invalid worker cleanup plan")
        }
        Ok(())
    }
}
fn root() -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/tmp")?;
    let metadata = file.metadata()?;
    if metadata.uid() != 0 || metadata.mode() & 0o1777 != 0o1777 {
        bail!("Worker cleanup requires the root-owned sticky temporary directory")
    }
    Ok(file)
}
fn stat_at(parent: &File, name: &std::ffi::CStr) -> Result<Option<libc::stat>> {
    let mut stat = std::mem::MaybeUninit::uninit();
    // The parent descriptor stays open; final symlinks are never followed.
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(None);
        }
        return Err(error.into());
    }
    Ok(Some(unsafe { stat.assume_init() }))
}
struct Checked {
    directory: File,
    name: std::ffi::CString,
    socket_present: bool,
}
fn inspect(root: &File, record: &SocketRecord) -> Result<Option<Checked>> {
    record.validate()?;
    let name = std::ffi::CString::new(record.name.as_str())?;
    let Some(stat) = stat_at(root, &name)? else {
        return Ok(None);
    };
    if stat.st_mode & libc::S_IFMT != libc::S_IFDIR
        || stat.st_uid != record.uid
        || stat.st_mode & 0o777 != 0o700
        || stat.st_dev != record.directory_device
        || stat.st_ino != record.directory_inode
    {
        bail!("Worker directory changed; refuse cleanup")
    }
    let fd = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let directory = unsafe { File::from_raw_fd(fd) };
    let metadata = directory.metadata()?;
    if metadata.dev() != record.directory_device || metadata.ino() != record.directory_inode {
        bail!("Worker directory replaced during inspection")
    }
    // Linux procfs resolves this already-open directory, not the mutable /tmp path.
    for (index, entry) in
        std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))?.enumerate()
    {
        if index >= 1 || entry?.file_name() != "worker.sock" {
            bail!("Worker directory contains unapproved entries")
        }
    }
    let socket_present = if let Some(socket) = stat_at(&directory, c"worker.sock")? {
        if socket.st_mode & libc::S_IFMT != libc::S_IFSOCK
            || socket.st_uid != record.uid
            || socket.st_mode & 0o777 != 0o600
            || socket.st_nlink != 1
            || socket.st_dev != record.socket_device
            || socket.st_ino != record.socket_inode
        {
            bail!("Worker socket changed; refuse cleanup")
        }
        true
    } else {
        false
    };
    Ok(Some(Checked {
        directory,
        name,
        socket_present,
    }))
}
fn unlink(parent: &File, name: &std::ffi::CStr, flags: i32) -> Result<()> {
    if unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
impl Store {
    fn cleanup_lease(&self) -> Result<ExecutionLease> {
        security::authorize("human-cli", "worker.cleanup")?;
        let path = self
            .runtime_lock
            .as_deref()
            .context("Worker cleanup requires a persistent store")?;
        ExecutionLease::acquire(path, true)
            .context("Cannot clean worker files while a runtime lease is held")
    }
    fn worker_files_plan(
        &self,
        lease: &ExecutionLease,
        after: u64,
        limit: u32,
    ) -> Result<CleanupPlan> {
        if after > i64::MAX as u64 || !(1..=128).contains(&limit) {
            bail!("Invalid worker cleanup page")
        }
        let started = std::time::Instant::now();
        let metadata = lease.0.metadata()?;
        let mut plan = CleanupPlan {
            workspace: (metadata.dev(), metadata.ino()),
            after,
            limit,
            cursor: after,
            more: false,
            entries: Vec::new(),
            checksum: String::new(),
        };
        let db = self.lock()?;
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > std::time::Duration::from_secs(5)),
        );
        let result = (|| -> Result<_> {
            Ok(db.prepare("SELECT id,task_id,CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload ELSE NULL END FROM events WHERE kind='worker_socket' AND source='fabric' AND id>?1 ORDER BY id LIMIT ?2")?
            .query_map(rusqlite::params![after, limit+1], |r| Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
        })();
        db.progress_handler(0, None::<fn() -> bool>);
        let rows = result?;
        drop(db);
        let root = root()?;
        let mut seen = std::collections::BTreeSet::new();
        for (index, (event_id, task_id, raw)) in rows.into_iter().enumerate() {
            if index == limit as usize {
                plan.more = true;
                break;
            }
            if started.elapsed() > std::time::Duration::from_secs(5) {
                bail!("Worker cleanup preview deadline exhausted")
            }
            uuid::Uuid::parse_str(&task_id)?;
            let resource: SocketRecord =
                serde_json::from_str(&raw.context("Worker socket journal exceeds bounds")?)?;
            if let Some(checked) = inspect(&root, &resource)? {
                if !seen.insert((resource.directory_device, resource.directory_inode)) {
                    bail!("Duplicate worker socket journal entry")
                }
                plan.entries.push(Entry {
                    event_id,
                    task_id,
                    resource,
                    socket_present: checked.socket_present,
                });
            }
            plan.cursor = event_id;
        }
        plan.checksum = plan.digest()?;
        Ok(plan)
    }
    pub fn preview_worker_files(&self, after: u64, limit: u32) -> Result<CleanupPlan> {
        let lease = self.cleanup_lease()?;
        self.worker_files_plan(&lease, after, limit)
    }
    pub fn clean_worker_files(&self, plan: &CleanupPlan, human_approve: bool) -> Result<usize> {
        if !human_approve {
            bail!("Worker cleanup requires explicit --human-approve")
        }
        plan.validate()?;
        let lease = self.cleanup_lease()?;
        if *plan != self.worker_files_plan(&lease, plan.after, plan.limit)? {
            bail!("Worker cleanup plan is stale; review a fresh preview")
        }
        let root = root()?;
        let started = std::time::Instant::now();
        let mut removed = 0;
        for entry in &plan.entries {
            if started.elapsed() > std::time::Duration::from_secs(5) {
                bail!("Worker cleanup deadline exhausted")
            }
            inspect(&root, &entry.resource)?.context("Worker files disappeared after preview")?;
            self.event(&entry.task_id, "worker_socket_cleanup", "human-cli", serde_json::json!({"state":"intent","event_id":entry.event_id,"plan_checksum":plan.checksum,"resource":entry.resource}))?;
            // Persistence may block; validate again before destructive operations.
            let checked = inspect(&root, &entry.resource)?
                .context("Worker files disappeared during cleanup audit")?;
            if checked.socket_present {
                unlink(&checked.directory, c"worker.sock", 0)?;
            }
            // Recheck the parent entry before removing only this now-empty directory.
            let stat = stat_at(&root, &checked.name)?
                .context("Worker directory disappeared during cleanup")?;
            if stat.st_dev != entry.resource.directory_device
                || stat.st_ino != entry.resource.directory_inode
            {
                bail!("Worker directory replaced during cleanup")
            }
            unlink(&root, &checked.name, libc::AT_REMOVEDIR)?;
            removed += 1;
            self.event(&entry.task_id, "worker_socket_cleanup", "human-cli", serde_json::json!({"state":"removed","event_id":entry.event_id,"plan_checksum":plan.checksum}))?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
#[path = "worker_files_tests.rs"]
mod tests;
