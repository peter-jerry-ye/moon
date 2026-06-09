// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::ffi::OsString;

use crate::async_host::types::Platform;
use crate::async_sys::ported_fns;

pub(crate) type HostHandle = i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct GuestBuffer {
    ptr: i32,
    offset: i32,
    len: i32,
}

impl GuestBuffer {
    #[allow(dead_code)]
    pub(crate) fn new(ptr: i32, offset: i32, len: i32) -> Self {
        Self { ptr, offset, len }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct OpenJobResult {
    fd: HostHandle,
    kind: i32,
    dev_id: u64,
    file_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct Job {
    ret: i64,
    err: i32,
    payload: JobPayload,
}

impl Job {
    #[allow(dead_code)]
    fn new(payload: JobPayload) -> Self {
        Self {
            ret: 0,
            err: 0,
            payload,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn run(&mut self) {
        self.ret = 0;
        self.err = 0;

        match &mut self.payload {
            JobPayload::Sleep { duration_ms } => run_sleep_job(*duration_ms),
            _ => self.err = unsupported_job_errno(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum JobPayload {
    Sleep {
        duration_ms: i32,
    },
    Read {
        fd: HostHandle,
        dst: GuestBuffer,
        position: i64,
    },
    Write {
        fd: HostHandle,
        data: Vec<u8>,
        position: i64,
    },
    Open {
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
        result: Option<OpenJobResult>,
    },
    KindOfFd {
        fd: HostHandle,
    },
    FileKindByPath {
        parent: HostHandle,
        path: OsString,
        follow_symlink: bool,
    },
    FileSize {
        fd: HostHandle,
        result: i64,
    },
    FileTime {
        fd: HostHandle,
        out: GuestBuffer,
    },
    FileTimeByPath {
        path: OsString,
        out: GuestBuffer,
        follow_symlink: bool,
    },
    Access {
        path: OsString,
        access: i32,
    },
    Chmod {
        path: OsString,
        mode: i32,
    },
    Fsync {
        fd: HostHandle,
        only_data: bool,
    },
    Flock {
        fd: HostHandle,
        exclusive: bool,
    },
    Remove {
        path: OsString,
    },
    Rename {
        old_path: OsString,
        new_path: OsString,
        replace: bool,
    },
    Symlink {
        target: OsString,
        path: OsString,
    },
    Mkdir {
        path: OsString,
        mode: i32,
    },
    Rmdir {
        path: OsString,
    },
    Readdir {
        dir: HostHandle,
        dst: GuestBuffer,
        restart: bool,
    },
    Realpath {
        path: OsString,
        result: Option<OsString>,
    },
    WaitForProcess {
        pid_or_handle: HostHandle,
    },
    Bind {
        socket: HostHandle,
        addr: Vec<u8>,
    },
    GetAddrInfo {
        hostname: OsString,
    },
    Sigwait {
        signals: Vec<i32>,
    },
    InotifyAddWatch {
        inotify: HostHandle,
        path: OsString,
        is_dir: bool,
    },
}

#[cfg(unix)]
#[allow(dead_code)]
pub(crate) type WorkerThreadId = libc::pthread_t;

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) type WorkerThreadId = windows_sys::Win32::Foundation::HANDLE;

#[allow(dead_code)]
pub(crate) struct Worker {
    id: Option<WorkerThreadId>,
    job_id: i32,
    job: Option<Job>,
    waiting: bool,
    wakeup: WorkerWakeup,
}

impl Worker {
    #[allow(dead_code)]
    pub(crate) fn new(init_job_id: i32, init_job: Job) -> Self {
        Self {
            id: None,
            job_id: init_job_id,
            job: Some(init_job),
            waiting: false,
            wakeup: WorkerWakeup::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn wake(&mut self, job_id: i32, job: Option<Job>) {
        self.job_id = job_id;
        self.job = job;
        self.wakeup.wake(self.id, &mut self.waiting);
    }

    #[allow(dead_code)]
    pub(crate) fn enter_idle(&mut self) {
        self.job = None;
    }

    #[allow(dead_code)]
    pub(crate) fn mark_waiting(&mut self) {
        self.waiting = true;
    }

    #[allow(dead_code)]
    pub(crate) fn wait_for_wake(&mut self) {
        self.wakeup.wait(&mut self.waiting);
    }

    #[allow(dead_code)]
    pub(crate) fn cancel(&self) -> i32 {
        if self.waiting {
            return 1;
        }
        cancel_running_worker(self.id)
    }
}

#[cfg(windows)]
struct WorkerWakeup {
    event: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WorkerWakeup {
    fn new() -> Self {
        use windows_sys::Win32::System::Threading::CreateEventA;

        let event = unsafe { CreateEventA(std::ptr::null(), 0, 0, std::ptr::null()) };
        Self { event }
    }

    fn wake(&self, _id: Option<WorkerThreadId>, waiting: &mut bool) {
        use windows_sys::Win32::System::Threading::SetEvent;

        *waiting = false;
        unsafe {
            SetEvent(self.event);
        }
    }

    fn wait(&self, _waiting: &mut bool) {
        use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

        unsafe {
            WaitForSingleObject(self.event, INFINITE);
        }
    }
}

#[cfg(windows)]
impl Drop for WorkerWakeup {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.event);
        }
    }
}

#[cfg(all(unix, target_os = "macos"))]
struct WorkerWakeup {
    mutex: libc::pthread_mutex_t,
    cond: libc::pthread_cond_t,
}

#[cfg(all(unix, target_os = "macos"))]
impl WorkerWakeup {
    fn new() -> Self {
        let mut mutex = std::mem::MaybeUninit::<libc::pthread_mutex_t>::uninit();
        let mut cond = std::mem::MaybeUninit::<libc::pthread_cond_t>::uninit();
        unsafe {
            libc::pthread_mutex_init(mutex.as_mut_ptr(), std::ptr::null());
            libc::pthread_cond_init(cond.as_mut_ptr(), std::ptr::null());
            Self {
                mutex: mutex.assume_init(),
                cond: cond.assume_init(),
            }
        }
    }

    fn wake(&mut self, _id: Option<WorkerThreadId>, waiting: &mut bool) {
        unsafe {
            libc::pthread_mutex_lock(&mut self.mutex);
            *waiting = false;
            libc::pthread_cond_signal(&mut self.cond);
            libc::pthread_mutex_unlock(&mut self.mutex);
        }
    }

    fn wait(&mut self, waiting: &mut bool) {
        unsafe {
            libc::pthread_mutex_lock(&mut self.mutex);
            while *waiting {
                // Keep parity with async's native macOS workaround: retry
                // pthread_cond_wait when it spuriously reports EINVAL.
                while libc::pthread_cond_wait(&mut self.cond, &mut self.mutex) == libc::EINVAL {}
            }
            libc::pthread_mutex_unlock(&mut self.mutex);
        }
    }
}

#[cfg(all(unix, target_os = "macos"))]
impl Drop for WorkerWakeup {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_mutex_destroy(&mut self.mutex);
            libc::pthread_cond_destroy(&mut self.cond);
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct WorkerWakeup;

#[cfg(all(unix, not(target_os = "macos")))]
impl WorkerWakeup {
    fn new() -> Self {
        Self
    }

    fn wake(&self, id: Option<WorkerThreadId>, waiting: &mut bool) {
        *waiting = false;
        if let Some(id) = id {
            unsafe {
                libc::pthread_kill(id, libc::SIGUSR1);
            }
        }
    }

    fn wait(&self, _waiting: &mut bool) {
        let mut sig = 0;
        let mut wakeup_signal = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        unsafe {
            libc::sigemptyset(wakeup_signal.as_mut_ptr());
            let mut wakeup_signal = wakeup_signal.assume_init();
            libc::sigaddset(&mut wakeup_signal, libc::SIGUSR1);
            libc::sigwait(&wakeup_signal, &mut sig);
        }
    }
}

#[cfg(windows)]
fn cancel_running_worker(id: Option<WorkerThreadId>) -> i32 {
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::System::Threading::CancelSynchronousIo;

    let Some(id) = id else {
        return 0;
    };
    if unsafe { CancelSynchronousIo(id) } != 0 {
        1
    } else if unsafe { GetLastError() } == ERROR_NOT_FOUND {
        0
    } else {
        -1
    }
}

#[cfg(unix)]
fn cancel_running_worker(id: Option<WorkerThreadId>) -> i32 {
    let Some(id) = id else {
        return 0;
    };
    unsafe {
        libc::pthread_kill(id, libc::SIGUSR2);
    }
    0
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_platform"
    )]
    pub(crate) fn get_platform() -> Platform {
        #[cfg(windows)]
        {
            Platform::Windows
        }
        #[cfg(all(unix, target_os = "macos"))]
        {
            Platform::MacOS
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Platform::Linux
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_ret"
    )]
    #[allow(dead_code)]
    pub(crate) fn job_get_ret(job: &Job) -> i64 {
        job.ret
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_err"
    )]
    #[allow(dead_code)]
    pub(crate) fn job_get_err(job: &Job) -> i32 {
        job.err
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_errno_is_cancelled"
    )]
    pub(crate) fn errno_is_cancelled(errno: i32) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED;
            errno == ERROR_OPERATION_ABORTED as i32
        }
        #[cfg(unix)]
        {
            errno == libc::EINTR
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_sleep_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_sleep_job(ms: i32) -> Job {
        Job::new(JobPayload::Sleep { duration_ms: ms })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_read_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_read_job(
        fd: HostHandle,
        ptr: i32,
        offset: i32,
        len: i32,
        position: i64,
    ) -> Job {
        Job::new(JobPayload::Read {
            fd,
            dst: GuestBuffer::new(ptr, offset, len),
            position,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_write_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_write_job(fd: HostHandle, data: Vec<u8>, position: i64) -> Job {
        Job::new(JobPayload::Write { fd, data, position })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_open_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_open_job(
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
    ) -> Job {
        Job::new(JobPayload::Open {
            filename,
            access,
            create_mode,
            append,
            sync,
            mode,
            result: None,
        })
    }
}

#[allow(dead_code)]
fn run_sleep_job(duration_ms: i32) {
    #[cfg(windows)]
    {
        // Match the native stub's `Sleep(((struct sleep_job*)job)->duration)`.
        unsafe { windows_sys::Win32::System::Threading::Sleep(duration_ms as u32) };
    }
    #[cfg(all(unix, target_os = "macos"))]
    {
        run_sleep_job_with_kqueue(duration_ms);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_sleep_job_with_nanosleep(duration_ms);
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn run_sleep_job_with_kqueue(duration_ms: i32) {
    let kqfd = unsafe { libc::kqueue() };
    let duration = sleep_job_timespec(duration_ms);
    let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();

    // Native async intentionally uses kqueue as a timeout-only sleeper on
    // macOS because nanosleep was too imprecise on CI runners.
    unsafe {
        libc::kevent(kqfd, std::ptr::null(), 0, event.as_mut_ptr(), 1, &duration);
        libc::close(kqfd);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run_sleep_job_with_nanosleep(duration_ms: i32) {
    let duration = sleep_job_timespec(duration_ms);
    unsafe {
        libc::nanosleep(&duration, std::ptr::null_mut());
    }
}

#[cfg(unix)]
fn sleep_job_timespec(duration_ms: i32) -> libc::timespec {
    libc::timespec {
        tv_sec: (duration_ms / 1000) as libc::time_t,
        tv_nsec: ((duration_ms % 1000) * 1_000_000) as libc::c_long,
    }
}

fn unsupported_job_errno() -> i32 {
    #[cfg(unix)]
    {
        libc::ENOSYS
    }
    #[cfg(windows)]
    {
        windows_sys::Win32::Foundation::ERROR_CALL_NOT_IMPLEMENTED as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_job_initial_result_matches_native_job_header() {
        let job = make_sleep_job(0);

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[test]
    fn read_job_carries_host_handle_and_guest_buffer_payload() {
        let job = make_read_job(7, 100, 2, 8, -1);

        assert_eq!(
            job.payload,
            JobPayload::Read {
                fd: 7,
                dst: GuestBuffer::new(100, 2, 8),
                position: -1
            }
        );
    }

    #[test]
    fn open_job_carries_owned_path_and_open_flags() {
        let job = make_open_job(OsString::from("/tmp/example"), 2, 3, true, 1, 0o644);

        assert_eq!(
            job.payload,
            JobPayload::Open {
                filename: OsString::from("/tmp/example"),
                access: 2,
                create_mode: 3,
                append: true,
                sync: 1,
                mode: 0o644,
                result: None
            }
        );
    }

    #[test]
    fn sleep_job_runs_without_error() {
        let mut job = make_sleep_job(0);

        job.run();

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[test]
    fn worker_wake_replaces_job_and_leaves_waiting_state() {
        let mut worker = Worker::new(1, make_sleep_job(0));
        worker.mark_waiting();

        worker.wake(2, Some(make_sleep_job(0)));

        assert_eq!(worker.job_id, 2);
        assert!(worker.job.is_some());
        assert!(!worker.waiting);
    }

    #[test]
    fn worker_enter_idle_clears_current_job() {
        let mut worker = Worker::new(1, make_sleep_job(0));

        worker.enter_idle();

        assert!(worker.job.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn unix_errno_is_cancelled_matches_async_stub() {
        assert!(errno_is_cancelled(libc::EINTR));
        assert!(!errno_is_cancelled(libc::EINVAL));
    }
}
