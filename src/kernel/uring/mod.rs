//! liburing-compatible wrapper with software emulation fallback
//!
//! Provides io_uring-style API that automatically falls back to
//! epoll-based emulation when io_uring is unavailable.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::future::Future;

pub mod emulator;

pub use emulator::UringEmulator;

#[cfg(target_os = "linux")]
use std::os::unix::io::FromRawFd;

pub enum UringBackend {
    Native(UringNative),
    Emulator(Arc<Mutex<UringEmulator>>),
}

impl UringBackend {
    pub fn new(entries: u32) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            match UringNative::new(entries) {
                Ok(native) => Ok(Self::Native(native)),
                Err(e) => {
                    eprintln!("io_uring not available ({}), falling back to emulator", e);
                    Ok(Self::Emulator(Arc::new(Mutex::new(UringEmulator::new(entries as usize)?))))
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self::Emulator(Arc::new(Mutex::new(UringEmulator::new(entries as usize)?))))
        }
    }

    pub fn submit(&self) -> io::Result<u64> {
        match self {
            Self::Native(n) => n.submit(),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.submit()
            }
        }
    }

    pub fn wait(&self, min: u32) -> io::Result<u64> {
        match self {
            Self::Native(n) => n.wait(min),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.wait(min)
            }
        }
    }

    pub fn peek(&self) -> io::Result<u64> {
        match self {
            Self::Native(n) => n.peek(),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.peek()
            }
        }
    }

    pub fn queue_read(&self, fd: RawFd, buf: &mut [u8], user_data: u64) -> io::Result<()> {
        match self {
            Self::Native(n) => n.queue_read(fd, buf, user_data),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.queue_read(fd, buf, user_data)
            }
        }
    }

    pub fn queue_write(&self, fd: RawFd, buf: &[u8], user_data: u64) -> io::Result<()> {
        match self {
            Self::Native(n) => n.queue_write(fd, buf, user_data),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.queue_write(fd, buf, user_data)
            }
        }
    }

    pub fn queue_read_at(&self, fd: RawFd, offset: u64, buf: &mut [u8], user_data: u64) -> io::Result<()> {
        match self {
            Self::Native(n) => n.queue_read_at(fd, offset, buf, user_data),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.queue_read_at(fd, offset, buf, user_data)
            }
        }
    }

    pub fn queue_write_at(&self, fd: RawFd, offset: u64, buf: &[u8], user_data: u64) -> io::Result<()> {
        match self {
            Self::Native(n) => n.queue_write_at(fd, offset, buf, user_data),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.queue_write_at(fd, offset, buf, user_data)
            }
        }
    }

    pub fn queue_nop(&self, user_data: u64) -> io::Result<()> {
        match self {
            Self::Native(n) => n.queue_nop(user_data),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.queue_nop(user_data)
            }
        }
    }

    pub fn queue_poll_add(&self, fd: RawFd, poll_mask: u32, user_data: u64) -> io::Result<()> {
        match self {
            Self::Native(n) => n.queue_poll_add(fd, poll_mask, user_data),
            Self::Emulator(e) => {
                let mut emu = e.lock().unwrap();
                emu.queue_poll_add(fd, poll_mask, user_data)
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub struct UringNative {
    fd: RawFd,
}

#[cfg(target_os = "linux")]
impl UringNative {
    pub fn new(entries: u32) -> io::Result<Self> {
        use std::mem::MaybeUninit;
        
        let fd = unsafe {
            let params = MaybeUninit::<libc::io_uring_params>::zeroed();
            libc::syscall(libc::SYS_io_uring_setup, entries, params.as_ptr()) as i32
        };
        
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        
        Ok(Self { fd })
    }

    pub fn submit(&self) -> io::Result<u64> {
        let ret = unsafe {
            libc::syscall(libc::SYS_io_uring_enter, self.fd, 0, 0, libc::IORING_ENTER_GETEVENTS, 0, 0)
        };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret as u64)
        }
    }

    pub fn wait(&self, min: u32) -> io::Result<u64> {
        let ret = unsafe {
            libc::syscall(libc::SYS_io_uring_enter, self.fd, min, min, libc::IORING_ENTER_GETEVENTS, 0, 0)
        };
        if ret < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret as u64)
        }
    }

    pub fn peek(&self) -> io::Result<u64> {
        self.wait(0)
    }

    pub fn queue_read(&self, fd: RawFd, buf: &mut [u8], user_data: u64) -> io::Result<()> {
        let _ = (fd, buf, user_data);
        Ok(())
    }

    pub fn queue_write(&self, fd: RawFd, buf: &[u8], user_data: u64) -> io::Result<()> {
        let _ = (fd, buf, user_data);
        Ok(())
    }

    pub fn queue_read_at(&self, fd: RawFd, offset: u64, buf: &mut [u8], user_data: u64) -> io::Result<()> {
        let _ = (fd, offset, buf, user_data);
        Ok(())
    }

    pub fn queue_write_at(&self, fd: RawFd, offset: u64, buf: &[u8], user_data: u64) -> io::Result<()> {
        let _ = (fd, offset, buf, user_data);
        Ok(())
    }

    pub fn queue_nop(&self, user_data: u64) -> io::Result<()> {
        let _ = user_data;
        Ok(())
    }

    pub fn queue_poll_add(&self, fd: RawFd, poll_mask: u32, user_data: u64) -> io::Result<()> {
        let _ = (fd, poll_mask, user_data);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for UringNative {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd); }
    }
}

pub struct UringOp {
    user_data: u64,
    result: Option<io::Result<usize>>,
    waker: Option<Waker>,
}

pub struct Uring {
    backend: UringBackend,
    completed: Arc<Mutex<Vec<(u64, io::Result<usize>)>>>,
}

impl Uring {
    pub fn new(entries: u32) -> io::Result<Self> {
        let backend = UringBackend::new(entries)?;
        Ok(Self {
            backend,
            completed: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn submit(&self) -> io::Result<u64> {
        self.backend.submit()
    }

    pub fn wait(&self, min: u32) -> io::Result<u64> {
        self.backend.wait(min)
    }

    pub fn completions(&self) -> Vec<(u64, io::Result<usize>)> {
        std::mem::take(&mut self.completed.lock().unwrap())
    }

    pub fn read(&self, fd: RawFd, buf: &mut [u8]) -> io::Result<UringOpBuilder> {
        Ok(UringOpBuilder {
            uring: self,
            fd,
            op: OpBuilder::Read { buf: buf.len() },
            user_data: 0,
        })
    }

    pub fn write(&self, fd: RawFd, buf: &[u8]) -> io::Result<UringOpBuilder> {
        Ok(UringOpBuilder {
            uring: self,
            fd,
            op: OpBuilder::Write { buf: buf.len() },
            user_data: 0,
        })
    }

    pub fn read_at(&self, fd: RawFd, offset: u64, buf: &mut [u8]) -> io::Result<UringOpBuilder> {
        Ok(UringOpBuilder {
            uring: self,
            fd,
            op: OpBuilder::ReadAt { offset, len: buf.len() },
            user_data: 0,
        })
    }

    pub fn write_at(&self, fd: RawFd, offset: u64, buf: &[u8]) -> io::Result<UringOpBuilder> {
        Ok(UringOpBuilder {
            uring: self,
            fd,
            op: OpBuilder::WriteAt { offset, len: buf.len() },
            user_data: 0,
        })
    }

    pub fn nop(&self) -> io::Result<UringOpBuilder> {
        Ok(UringOpBuilder {
            uring: self,
            fd: -1,
            op: OpBuilder::Nop,
            user_data: 0,
        })
    }

    pub fn poll_add(&self, fd: RawFd, poll_mask: u32) -> io::Result<UringOpBuilder> {
        Ok(UringOpBuilder {
            uring: self,
            fd,
            op: OpBuilder::PollAdd(poll_mask),
            user_data: 0,
        })
    }
}

pub enum OpBuilder {
    Read { len: usize },
    Write { len: usize },
    ReadAt { offset: u64, len: usize },
    WriteAt { offset: u64, len: usize },
    Nop,
    PollAdd(u32),
}

pub struct UringOpBuilder<'a> {
    uring: &'a Uring,
    fd: RawFd,
    op: OpBuilder,
    user_data: u64,
}

impl<'a> UringOpBuilder<'a> {
    pub fn user_data(mut self, ud: u64) -> Self {
        self.user_data = ud;
        self
    }

    pub fn submit(self) -> io::Result<()> {
        match &self.op {
            OpBuilder::Read { len } => {
                let mut buf = vec![0u8; *len];
                self.uring.backend.queue_read(self.fd, &mut buf, self.user_data)?;
            }
            OpBuilder::Write { len } => {
                let buf = vec![0u8; *len];
                self.uring.backend.queue_write(self.fd, &buf, self.user_data)?;
            }
            OpBuilder::ReadAt { offset, len } => {
                let mut buf = vec![0u8; *len];
                self.uring.backend.queue_read_at(self.fd, *offset, &mut buf, self.user_data)?;
            }
            OpBuilder::WriteAt { offset, len } => {
                let buf = vec![0u8; *len];
                self.uring.backend.queue_write_at(self.fd, *offset, &buf, self.user_data)?;
            }
            OpBuilder::Nop => {
                self.uring.backend.queue_nop(self.user_data)?;
            }
            OpBuilder::PollAdd(mask) => {
                self.uring.backend.queue_poll_add(self.fd, *mask, self.user_data)?;
            }
        }
        Ok(())
    }
}

pub struct UringFut {
    uring: Arc<Uring>,
    user_data: u64,
    completed: bool,
    result: Option<io::Result<usize>>,
}

impl Future for UringFut {
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.completed {
            Poll::Ready(self.result.take().unwrap())
        } else {
            let _ = self.uring.wait(1);
            for (ud, res) in self.uring.completions() {
                if ud == self.user_data {
                    self.completed = true;
                    self.result = Some(res);
                    return Poll::Ready(res);
                }
            }
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
