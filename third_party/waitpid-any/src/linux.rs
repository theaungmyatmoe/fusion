use std::io::{Error, ErrorKind, Result};
use std::time::Duration;

use rustix::event::{poll, PollFd, PollFlags};
use rustix::io::Errno;

pub type WaitHandle = rustix::fd::OwnedFd;

fn raw_pidfd_open(pid: i32, flags: u32) -> Result<i32> {
    unsafe {
        let fd = libc::syscall(libc::SYS_pidfd_open, pid, flags);
        if fd < 0 {
            return Err(Error::last_os_error());
        }
        Ok(fd as i32)
    }
}

pub fn open(pid: i32) -> Result<WaitHandle> {
    if pid <= 0 {
        return Err(Error::new(ErrorKind::InvalidInput, format!("invalid PID {pid}")));
    }
    let fd = raw_pidfd_open(pid, 0)?;
    unsafe {
        use std::os::unix::io::FromRawFd;
        Ok(rustix::fd::OwnedFd::from_raw_fd(fd))
    }
}

pub fn wait(pidfd: &mut WaitHandle, timeout: Option<Duration>) -> Result<Option<()>> {
    let timespec = match timeout {
        Some(dur) => Some(dur.try_into().map_err(|_| Errno::INVAL)?),
        // Infinite.
        None => None,
    };
    let mut fds = [PollFd::new(&pidfd, PollFlags::IN)];
    let ret = poll(&mut fds, timespec.as_ref())?;
    if ret == 0 {
        // Timeout.
        return Ok(None);
    }
    debug_assert!(fds[0].revents().contains(PollFlags::IN));
    Ok(Some(()))
}
