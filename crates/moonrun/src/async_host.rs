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

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(not(any(unix, windows)))]
compile_error!("moonrun async wasm host currently supports only Unix and Windows hosts");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncHostError {
    Fault,
    Inval,
    #[allow(dead_code)]
    Badf,
    NotSupported,
}

pub(crate) type AsyncHostResult<T> = Result<T, AsyncHostError>;

#[cfg(unix)]
mod native_errno {
    pub(crate) const BADF: i32 = libc::EBADF;
    pub(crate) const FAULT: i32 = libc::EFAULT;
    pub(crate) const INVAL: i32 = libc::EINVAL;
    pub(crate) const NOT_SUPPORTED: i32 = libc::ENOSYS;
}

#[cfg(windows)]
mod native_errno {
    use windows_sys::Win32::Foundation::{
        ERROR_CALL_NOT_IMPLEMENTED, ERROR_INVALID_ADDRESS, ERROR_INVALID_HANDLE,
        ERROR_INVALID_PARAMETER,
    };

    pub(crate) const BADF: i32 = ERROR_INVALID_HANDLE as i32;
    pub(crate) const FAULT: i32 = ERROR_INVALID_ADDRESS as i32;
    pub(crate) const INVAL: i32 = ERROR_INVALID_PARAMETER as i32;
    pub(crate) const NOT_SUPPORTED: i32 = ERROR_CALL_NOT_IMPLEMENTED as i32;
}

impl AsyncHostError {
    pub(crate) fn errno(self) -> i32 {
        match self {
            Self::Fault => native_errno::FAULT,
            Self::Inval => native_errno::INVAL,
            Self::Badf => native_errno::BADF,
            Self::NotSupported => native_errno::NOT_SUPPORTED,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestRange {
    offset: usize,
    len: usize,
}

impl GuestRange {
    pub(crate) fn new(offset: i32, len: i32) -> AsyncHostResult<Self> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        Ok(Self { offset, len })
    }

    fn end(self) -> AsyncHostResult<usize> {
        self.offset
            .checked_add(self.len)
            .ok_or(AsyncHostError::Fault)
    }
}

#[allow(dead_code)]
pub(crate) trait GuestMemory {
    fn bytes(&self) -> &[u8];

    fn bytes_mut(&mut self) -> &mut [u8];

    fn read(&self, range: GuestRange) -> AsyncHostResult<&[u8]> {
        let end = range.end()?;
        self.bytes()
            .get(range.offset..end)
            .ok_or(AsyncHostError::Fault)
    }

    fn write(&mut self, range: GuestRange, data: &[u8]) -> AsyncHostResult<()> {
        if range.len != data.len() {
            return Err(AsyncHostError::Inval);
        }
        let end = range.end()?;
        let dst = self
            .bytes_mut()
            .get_mut(range.offset..end)
            .ok_or(AsyncHostError::Fault)?;
        dst.copy_from_slice(data);
        Ok(())
    }

    fn fill(&mut self, range: GuestRange, value: u8) -> AsyncHostResult<()> {
        let end = range.end()?;
        let dst = self
            .bytes_mut()
            .get_mut(range.offset..end)
            .ok_or(AsyncHostError::Fault)?;
        dst.fill(value);
        Ok(())
    }

    fn read_i32_le(&self, offset: i32) -> AsyncHostResult<i32> {
        let bytes = self.read(GuestRange::new(offset, 4)?)?;
        Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn write_i32_le(&mut self, offset: i32, value: i32) -> AsyncHostResult<()> {
        self.write(GuestRange::new(offset, 4)?, &value.to_le_bytes())
    }
}

impl GuestMemory for [u8] {
    fn bytes(&self) -> &[u8] {
        self
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<const N: usize> GuestMemory for [u8; N] {
    fn bytes(&self) -> &[u8] {
        self.as_slice()
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostResourceKind {
    File,
    Poll,
    Job,
    Worker,
    IoResult,
    RawFd,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct HostResource {
    kind: HostResourceKind,
}

#[allow(dead_code)]
impl HostResource {
    pub(crate) fn new(kind: HostResourceKind) -> Self {
        Self { kind }
    }

    pub(crate) fn kind(&self) -> HostResourceKind {
        self.kind
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct ResourceSlot {
    generation: u16,
    resource: Option<HostResource>,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
struct ResourceTable {
    slots: Vec<ResourceSlot>,
}

#[allow(dead_code)]
impl ResourceTable {
    fn insert(&mut self, resource: HostResource) -> AsyncHostResult<i32> {
        if let Some((index, slot)) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.resource.is_none())
        {
            slot.resource = Some(resource);
            return encode_handle(index, slot.generation);
        }

        let index = self.slots.len();
        self.slots.push(ResourceSlot {
            generation: 1,
            resource: Some(resource),
        });
        encode_handle(index, 1)
    }

    fn get(&self, handle: i32) -> AsyncHostResult<&HostResource> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        slot.resource.as_ref().ok_or(AsyncHostError::Badf)
    }

    fn remove(&mut self, handle: i32) -> AsyncHostResult<HostResource> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        let resource = slot.resource.take().ok_or(AsyncHostError::Badf)?;
        slot.generation = next_generation(slot.generation);
        Ok(resource)
    }
}

#[allow(dead_code)]
fn encode_handle(index: usize, generation: u16) -> AsyncHostResult<i32> {
    if index >= 0x1_0000 {
        return Err(AsyncHostError::Fault);
    }
    Ok(((i32::from(generation)) << 16) | i32::try_from(index).unwrap())
}

#[allow(dead_code)]
fn decode_handle(handle: i32) -> AsyncHostResult<(usize, u16)> {
    if handle <= 0 {
        return Err(AsyncHostError::Badf);
    }
    let index = (handle as u32 & 0xffff) as usize;
    let generation = ((handle as u32 >> 16) & 0xffff) as u16;
    if generation == 0 {
        return Err(AsyncHostError::Badf);
    }
    Ok((index, generation))
}

#[allow(dead_code)]
fn next_generation(generation: u16) -> u16 {
    match generation.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingGuestWrite {
    dst: GuestRange,
    data: Vec<u8>,
}

#[allow(dead_code)]
impl PendingGuestWrite {
    pub(crate) fn new(dst: GuestRange, data: Vec<u8>) -> Self {
        Self { dst, data }
    }

    pub(crate) fn complete(self, memory: &mut (impl GuestMemory + ?Sized)) -> AsyncHostResult<()> {
        memory.write(self.dst, &self.data)
    }
}

#[derive(Default)]
struct AsyncHostState {
    errno: i32,
    #[allow(dead_code)]
    resources: ResourceTable,
}

#[derive(Default)]
pub(crate) struct AsyncHost {
    state: Mutex<AsyncHostState>,
}

impl AsyncHost {
    pub(crate) fn platform(&self) -> i32 {
        #[cfg(windows)]
        {
            2
        }
        #[cfg(all(unix, target_os = "macos"))]
        {
            1
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            0
        }
    }

    pub(crate) fn ms_since_epoch(&self) -> i64 {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is before the Unix epoch");
        let unix_ms = duration.as_millis() as i64;
        #[cfg(windows)]
        {
            unix_ms + 11_644_473_600_000
        }
        #[cfg(not(windows))]
        {
            unix_ms
        }
    }

    pub(crate) fn sleep_ms(&self, duration_ms: i32) {
        if duration_ms > 0 {
            std::thread::sleep(Duration::from_millis(duration_ms as u64));
        }
    }

    pub(crate) fn get_errno(&self) -> i32 {
        self.state.lock().unwrap().errno
    }

    pub(crate) fn set_errno(&self, errno: i32) {
        self.state.lock().unwrap().errno = errno;
    }

    pub(crate) fn record_error(&self, error: AsyncHostError) -> i32 {
        let errno = error.errno();
        self.set_errno(errno);
        errno
    }

    pub(crate) fn unsupported_return(&self) -> i32 {
        self.record_error(AsyncHostError::NotSupported);
        -1
    }

    pub(crate) fn copy_from_guest_len(
        &self,
        memory: &(impl GuestMemory + ?Sized),
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<i32> {
        let len = memory.read(GuestRange::new(offset, len)?)?.len();
        i32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    pub(crate) fn zero_guest(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        memory.fill(GuestRange::new(offset, len)?, 0)
    }

    #[allow(dead_code)]
    pub(crate) fn insert_resource(&self, resource: HostResource) -> AsyncHostResult<i32> {
        self.state.lock().unwrap().resources.insert(resource)
    }

    #[allow(dead_code)]
    pub(crate) fn resource_kind(&self, handle: i32) -> AsyncHostResult<HostResourceKind> {
        Ok(self.state.lock().unwrap().resources.get(handle)?.kind())
    }

    #[allow(dead_code)]
    pub(crate) fn remove_resource(&self, handle: i32) -> AsyncHostResult<HostResource> {
        self.state.lock().unwrap().resources.remove(handle)
    }

    pub(crate) fn is_nonblocking_io_error(&self, errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EAGAIN || errno == libc::EINPROGRESS
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{ERROR_IO_INCOMPLETE, ERROR_IO_PENDING};
            errno == ERROR_IO_INCOMPLETE as i32 || errno == ERROR_IO_PENDING as i32
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn is_eintr(&self, errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EINTR
        }
        #[cfg(not(unix))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn is_enoent(&self, errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::ENOENT
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
            errno == ERROR_FILE_NOT_FOUND as i32 || errno == ERROR_PATH_NOT_FOUND as i32
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn is_eexist(&self, errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EEXIST
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS};
            errno == ERROR_FILE_EXISTS as i32 || errno == ERROR_ALREADY_EXISTS as i32
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn is_eacces(&self, errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EACCES
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
            errno == ERROR_ACCESS_DENIED as i32
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn is_econnrefused(&self, errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::ECONNREFUSED
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_CONNECTION_REFUSED;
            errno == ERROR_CONNECTION_REFUSED as i32
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn is_error_notify_enum_dir(&self, errno: i32) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_NOTIFY_ENUM_DIR;
            errno == ERROR_NOTIFY_ENUM_DIR as i32
        }
        #[cfg(not(windows))]
        {
            let _ = errno;
            false
        }
    }

    pub(crate) fn enotdir(&self) -> i32 {
        #[cfg(unix)]
        {
            libc::ENOTDIR
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_DIRECTORY;
            ERROR_DIRECTORY as i32
        }
        #[cfg(not(any(unix, windows)))]
        {
            native_errno::INVAL
        }
    }
}

#[allow(dead_code)]
pub(crate) fn checked_range(memory: &[u8], offset: i32, len: i32) -> AsyncHostResult<&[u8]> {
    memory.read(GuestRange::new(offset, len)?)
}

#[allow(dead_code)]
pub(crate) fn checked_mut_range(
    memory: &mut [u8],
    offset: i32,
    len: i32,
) -> AsyncHostResult<&mut [u8]> {
    let range = GuestRange::new(offset, len)?;
    let end = range.end()?;
    memory
        .get_mut(range.offset..end)
        .ok_or(AsyncHostError::Fault)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_range_accepts_in_bounds_access() {
        let memory = [1, 2, 3, 4];

        assert_eq!(checked_range(&memory, 1, 2).unwrap(), &[2, 3]);
        assert!(checked_range(&memory, 4, 0).unwrap().is_empty());
    }

    #[test]
    fn checked_range_rejects_out_of_bounds_access() {
        let memory = [0; 4];

        for (offset, len) in [(-1, 1), (0, -1), (3, 2), (i32::MAX, 1), (2, i32::MAX)] {
            assert_eq!(
                checked_range(&memory, offset, len),
                Err(AsyncHostError::Fault)
            );
        }
    }

    #[test]
    fn checked_mut_range_accepts_in_bounds_access() {
        let mut memory = [1, 2, 3, 4];

        checked_mut_range(&mut memory, 1, 2).unwrap().fill(9);

        assert_eq!(memory, [1, 9, 9, 4]);
    }

    #[test]
    fn checked_mut_range_rejects_out_of_bounds_access() {
        let mut memory = [0; 4];

        for (offset, len) in [(-1, 1), (0, -1), (3, 2), (i32::MAX, 1), (2, i32::MAX)] {
            assert_eq!(
                checked_mut_range(&mut memory, offset, len),
                Err(AsyncHostError::Fault)
            );
        }
    }

    #[test]
    fn guest_memory_reads_and_writes_fixed_little_endian_records() {
        let mut memory = [0; 8];

        memory.write_i32_le(2, 0x1020_3040).unwrap();

        assert_eq!(memory.read_i32_le(2).unwrap(), 0x1020_3040);
        assert_eq!(&memory[2..6], &[0x40, 0x30, 0x20, 0x10]);
        assert_eq!(memory.write_i32_le(6, 1), Err(AsyncHostError::Fault));
    }

    #[test]
    fn pending_guest_write_reacquires_current_memory() {
        let pending = PendingGuestWrite::new(GuestRange::new(4, 3).unwrap(), b"abc".to_vec());
        let mut grown_memory = vec![0; 16];

        pending.complete(grown_memory.as_mut_slice()).unwrap();

        assert_eq!(&grown_memory[4..7], b"abc");
    }

    #[test]
    fn resource_handles_reject_invalid_and_stale_values() {
        let host = AsyncHost::default();
        let handle = host
            .insert_resource(HostResource::new(HostResourceKind::File))
            .unwrap();

        assert_eq!(host.resource_kind(handle), Ok(HostResourceKind::File));
        assert_eq!(
            host.remove_resource(handle).unwrap().kind(),
            HostResourceKind::File
        );
        assert_eq!(host.resource_kind(handle), Err(AsyncHostError::Badf));
        assert!(matches!(host.remove_resource(0), Err(AsyncHostError::Badf)));
    }

    #[test]
    fn unsupported_records_native_errno() {
        let host = AsyncHost::default();

        assert_eq!(host.unsupported_return(), -1);
        assert_eq!(host.get_errno(), AsyncHostError::NotSupported.errno());
    }
}
