//! io_uring emulator using epoll for non-Linux platforms or fallback
//!
//! This provides a software emulation of io_uring operations using epoll,
//! allowing the same API to work across all platforms.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

#[cfg(target_os = "linux")]
use libc::epoll::{
    epoll_create1, epoll_ctl, epoll_wait, EPOLLERR, EPOLLIN, EPOLLOUT, EPOLL_CTL_ADD,
    EPOLL_CTL_MOD, EPOLL_IN, EPOLL_OUT,
};

#[cfg(target_os = "linux")]
const EPOLL_CLOEXEC: i32 = 0x80000;

#[cfg(not(target_os = "linux"))]
pub fn epoll_create1(_flags: i32) -> i32 {
    -1
}

#[cfg(not(target_os = "linux"))]
pub mod epoll_consts {
    pub const EPOLL_CTL_ADD: i32 = 1;
    pub const EPOLL_CTL_MOD: i32 = 3;
    pub const EPOLL_IN: u32 = 0x1;
    pub const EPOLL_OUT: u32 = 0x4;
    pub const EPOLLERR: u32 = 0x8;
    pub const EPOLLIN: u32 = 0x1;
    pub const EPOLLOUT: u32 = 0x4;
}

#[cfg(not(target_os = "linux"))]
use epoll_consts::*;

pub struct UringEmulator {
    entries: usize,
    pending_ops: Vec<PendingOp>,
    completed_ops: Vec<CompletedOp>,
    fd_ops: HashMap<RawFd, FdOpState>,
    #[cfg(target_os = "linux")]
    epoll_fd: Option<i32>,
    user_data_counter: u64,
}

struct PendingOp {
    fd: RawFd,
    op: OpType,
    user_data: u64,
    buf: Option<Vec<u8>>,
    offset: Option<u64>,
}

enum OpType {
    Read { len: usize },
    Write { len: usize },
    ReadAt { offset: u64, len: usize },
    WriteAt { offset: u64, len: usize },
    Nop,
    PollAdd(u32),
}

struct CompletedOp {
    user_data: u64,
    result: io::Result<usize>,
}

struct FdOpState {
    readable: bool,
    writable: bool,
    registered: bool,
}

impl UringEmulator {
    pub fn new(entries: usize) -> io::Result<Self> {
        let epoll_fd = if cfg!(target_os = "linux") {
            unsafe {
                let fd = epoll_create1(EPOLL_CLOEXEC);
                if fd < 0 {
                    None
                } else {
                    Some(fd)
                }
            }
        } else {
            None
        };

        Ok(Self {
            entries,
            pending_ops: Vec::new(),
            completed_ops: Vec::new(),
            fd_ops: HashMap::new(),
            epoll_fd,
            user_data_counter: 0,
        })
    }

    pub fn queue_read(&mut self, fd: RawFd, buf: &mut [u8], user_data: u64) -> io::Result<()> {
        let op = OpType::Read { len: buf.len() };
        self.pending_ops.push(PendingOp {
            fd,
            op,
            user_data,
            buf: Some(buf.to_vec()),
            offset: None,
        });
        self.register_fd(fd, true, false)?;
        Ok(())
    }

    pub fn queue_write(&mut self, fd: RawFd, buf: &[u8], user_data: u64) -> io::Result<()> {
        let op = OpType::Write { len: buf.len() };
        self.pending_ops.push(PendingOp {
            fd,
            op,
            user_data,
            buf: Some(buf.to_vec()),
            offset: None,
        });
        self.register_fd(fd, false, true)?;
        Ok(())
    }

    pub fn queue_read_at(
        &mut self,
        fd: RawFd,
        offset: u64,
        buf: &mut [u8],
        user_data: u64,
    ) -> io::Result<()> {
        let op = OpType::ReadAt {
            offset,
            len: buf.len(),
        };
        self.pending_ops.push(PendingOp {
            fd,
            op,
            user_data,
            buf: Some(buf.to_vec()),
            offset: Some(offset),
        });
        self.register_fd(fd, true, false)?;
        Ok(())
    }

    pub fn queue_write_at(
        &mut self,
        fd: RawFd,
        offset: u64,
        buf: &[u8],
        user_data: u64,
    ) -> io::Result<()> {
        let op = OpType::WriteAt {
            offset,
            len: buf.len(),
        };
        self.pending_ops.push(PendingOp {
            fd,
            op,
            user_data,
            buf: Some(buf.to_vec()),
            offset: Some(offset),
        });
        self.register_fd(fd, false, true)?;
        Ok(())
    }

    pub fn queue_nop(&mut self, user_data: u64) -> io::Result<()> {
        self.pending_ops.push(PendingOp {
            fd: -1,
            op: OpType::Nop,
            user_data,
            buf: None,
            offset: None,
        });
        Ok(())
    }

    pub fn queue_poll_add(&mut self, fd: RawFd, poll_mask: u32, user_data: u64) -> io::Result<()> {
        let readable = (poll_mask & EPOLLIN) != 0;
        let writable = (poll_mask & EPOLLOUT) != 0;
        self.register_fd(fd, readable, writable)?;
        let op = OpType::PollAdd(poll_mask);
        self.pending_ops.push(PendingOp {
            fd,
            op,
            user_data,
            buf: None,
            offset: None,
        });
        Ok(())
    }

    fn register_fd(&mut self, fd: RawFd, readable: bool, writable: bool) -> io::Result<()> {
        if let Some(epoll_fd) = self.epoll_fd {
            if let Some(state) = self.fd_ops.get_mut(&fd) {
                let mut events: u32 = EPOLLERR;
                if readable || state.readable {
                    events |= EPOLLIN;
                }
                if writable || state.writable {
                    events |= EPOLLOUT;
                }
                unsafe {
                    libc::epoll_ctl(
                        epoll_fd,
                        EPOLL_CTL_MOD,
                        fd,
                        &mut libc::epoll_event {
                            events,
                            data: libc::epoll_data_t { u64: fd as u64 },
                        },
                    );
                }
                state.readable = state.readable || readable;
                state.writable = state.writable || writable;
            } else {
                let mut events: u32 = EPOLLERR;
                if readable {
                    events |= EPOLLIN;
                }
                if writable {
                    events |= EPOLLOUT;
                }
                unsafe {
                    let ret = libc::epoll_ctl(
                        epoll_fd,
                        EPOLL_CTL_ADD,
                        fd,
                        &mut libc::epoll_event {
                            events,
                            data: libc::epoll_data_t { u64: fd as u64 },
                        },
                    );
                    if ret < 0 {
                        return Err(Error::last_os_error());
                    }
                }
                self.fd_ops.insert(
                    fd,
                    FdOpState {
                        readable,
                        writable,
                        registered: true,
                    },
                );
            }
        }
        Ok(())
    }

    pub fn submit(&mut self) -> io::Result<u64> {
        let count = self.pending_ops.len() as u64;

        for op in self.pending_ops.drain(..) {
            self.execute_op(op)?;
        }

        Ok(count)
    }

    fn execute_op(&mut self, op: PendingOp) -> io::Result<()> {
        match op.op {
            OpType::Read { len } => {
                let mut buf = vec![0u8; len];
                let result =
                    unsafe { libc::read(op.fd, buf.as_mut_ptr() as *mut libc::c_void, len) };
                if result < 0 {
                    let err = Error::last_os_error();
                    if err.kind() == ErrorKind::WouldBlock {
                        return Ok(());
                    }
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Err(err),
                    });
                } else {
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Ok(result as usize),
                    });
                }
            }
            OpType::Write { len } => {
                let buf = vec![0u8; len];
                let result =
                    unsafe { libc::write(op.fd, buf.as_ptr() as *const libc::c_void, len) };
                if result < 0 {
                    let err = Error::last_os_error();
                    if err.kind() == ErrorKind::WouldBlock {
                        return Ok(());
                    }
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Err(err),
                    });
                } else {
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Ok(result as usize),
                    });
                }
            }
            OpType::ReadAt { offset, len } => {
                let mut buf = vec![0u8; len];
                let result = unsafe {
                    libc::pread(
                        op.fd,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        len,
                        offset as libc::off_t,
                    )
                };
                if result < 0 {
                    let err = Error::last_os_error();
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Err(err),
                    });
                } else {
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Ok(result as usize),
                    });
                }
            }
            OpType::WriteAt { offset, len } => {
                let buf = vec![0u8; len];
                let result = unsafe {
                    libc::pwrite(
                        op.fd,
                        buf.as_ptr() as *const libc::c_void,
                        len,
                        offset as libc::off_t,
                    )
                };
                if result < 0 {
                    let err = Error::last_os_error();
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Err(err),
                    });
                } else {
                    self.completed_ops.push(CompletedOp {
                        user_data: op.user_data,
                        result: Ok(result as usize),
                    });
                }
            }
            OpType::Nop => {
                self.completed_ops.push(CompletedOp {
                    user_data: op.user_data,
                    result: Ok(0),
                });
            }
            OpType::PollAdd(_) => {
                self.completed_ops.push(CompletedOp {
                    user_data: op.user_data,
                    result: Ok(0),
                });
            }
        }
        Ok(())
    }

    pub fn wait(&mut self, min: u32) -> io::Result<u64> {
        if let Some(epoll_fd) = self.epoll_fd {
            let mut events = [0u8; std::mem::size_of::<libc::epoll_event>() * 64];
            let timeout = if min > 0 { -1 } else { 0 };

            let count = unsafe {
                libc::epoll_wait(
                    epoll_fd,
                    events.as_mut_ptr() as *mut libc::epoll_event,
                    64,
                    timeout,
                )
            };

            if count < 0 {
                return Err(Error::last_os_error());
            }

            for i in 0..count as usize {
                let event = unsafe { *((events.as_ptr() as *const libc::epoll_event).add(i)) };
                let fd = event.data.u64 as RawFd;
                let evts = event.events;

                if let Some(state) = self.fd_ops.get_mut(&fd) {
                    if evts & EPOLLIN != 0 {
                        state.readable = true;
                    }
                    if evts & EPOLLOUT != 0 {
                        state.writable = true;
                    }
                }
            }
        }

        self.process_ready_ops()?;
        Ok(self.completed_ops.len() as u64)
    }

    fn process_ready_ops(&mut self) -> io::Result<()> {
        let mut ready_indices = Vec::new();

        for (i, op) in self.pending_ops.iter().enumerate() {
            let ready = match &op.op {
                OpType::Read { .. } | OpType::ReadAt { .. } => {
                    if let Some(state) = self.fd_ops.get(&op.fd) {
                        state.readable
                    } else {
                        true
                    }
                }
                OpType::Write { .. } | OpType::WriteAt { .. } => {
                    if let Some(state) = self.fd_ops.get(&op.fd) {
                        state.writable
                    } else {
                        true
                    }
                }
                OpType::Nop | OpType::PollAdd(_) => true,
            };

            if ready {
                ready_indices.push(i);
            }
        }

        for i in ready_indices.into_iter().rev() {
            let op = self.pending_ops.remove(i);
            self.execute_op(op)?;
        }

        Ok(())
    }

    pub fn peek(&mut self) -> io::Result<u64> {
        self.wait(0)
    }

    pub fn pop_completed(&mut self) -> Option<(u64, io::Result<usize>)> {
        self.completed_ops.pop().map(|op| (op.user_data, op.result))
    }

    pub fn get_completions(&mut self) -> Vec<(u64, io::Result<usize>)> {
        std::mem::take(&mut self.completed_ops)
    }
}

impl Drop for UringEmulator {
    fn drop(&mut self) {
        if let Some(epoll_fd) = self.epoll_fd {
            unsafe {
                libc::close(epoll_fd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emulator_creation() {
        let emulator = UringEmulator::new(32).unwrap();
        assert_eq!(emulator.entries, 32);
    }

    #[test]
    fn test_emulator_queue_nop() {
        let mut emulator = UringEmulator::new(32).unwrap();
        emulator.queue_nop(123).unwrap();

        let count = emulator.submit().unwrap();
        assert_eq!(count, 1);

        let completions = emulator.get_completions();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].0, 123);
        assert!(completions[0].1.is_ok());
    }

    #[test]
    fn test_emulator_read_write_pipe() {
        let mut emulator = UringEmulator::new(32).unwrap();

        let mut fds = [0i32; 2];
        unsafe {
            libc::pipe(fds.as_mut_ptr());
        }

        let test_data = b"hello world";

        emulator.queue_write(fds[1], test_data, 1).unwrap();
        emulator.submit().unwrap();

        let mut buf = vec![0u8; 64];
        emulator.queue_read(fds[0], &mut buf, 2).unwrap();
        emulator.submit().unwrap();

        let completions = emulator.get_completions();
        assert_eq!(completions.len(), 2);

        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    #[test]
    fn test_emulator_poll_add() {
        let mut emulator = UringEmulator::new(32).unwrap();

        let mut fds = [0i32; 2];
        unsafe {
            libc::pipe(fds.as_mut_ptr());
        }

        emulator.queue_poll_add(fds[0], EPOLLIN, 1).unwrap();
        emulator.queue_poll_add(fds[1], EPOLLOUT, 2).unwrap();

        let count = emulator.submit().unwrap();
        assert_eq!(count, 2);

        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }

    #[test]
    fn test_emulator_read_at() {
        use std::fs::{self, File};
        use std::io::Seek;

        let mut emulator = UringEmulator::new(32).unwrap();

        let tmpdir = std::env::temp_dir();
        let tmpfile = tmpdir.join("uring_test.txt");
        fs::write(&tmpfile, "hello world").unwrap();

        let file = File::open(&tmpfile).unwrap();
        let fd = file.as_raw_fd();

        let mut buf = vec![0u8; 5];
        emulator.queue_read_at(fd, 0, &mut buf, 1).unwrap();
        emulator.submit().unwrap();

        let completions = emulator.get_completions();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].1.unwrap(), 5);

        let _ = file;
        let _ = fs::remove_file(tmpfile);
    }
}
