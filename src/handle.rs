// Basic handle management for Unix file descriptors.
// Provides an OwnedHandle that owns a RawFd and closes it on Drop.

#![allow(dead_code)]

#[cfg(unix)]
mod unix {
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
    use std::sync::Arc;

    /// A safe, owning wrapper around a RawFd. Closes the fd when dropped.
    /// Can be converted to/from std::fs::File for read/write operations.
    #[derive(Debug)]
    pub struct OwnedHandle {
        fd: Arc<i32>,
    }

    impl OwnedHandle {
        /// Create from a raw fd. Takes ownership (will close on Drop).
        ///
        /// # Safety
        /// Caller must ensure fd is valid and not already closed.
        pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
            Self { fd: Arc::new(fd) }
        }

        /// Duplicate the handle, returning a new OwnedHandle that owns a separate duplicate fd.
        pub fn try_clone(&self) -> io::Result<Self> {
            let fd = *self.fd;
            // Use libc dup to duplicate fd
            let new_fd = unsafe { libc::dup(fd) };
            if new_fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(unsafe { OwnedHandle::from_raw_fd(new_fd) })
            }
        }

        /// Return the raw fd without closing it (consumes the OwnedHandle)
        /// To avoid moving out of a Drop type, duplicate the fd and return the duplicate.
        pub fn into_raw_fd(self) -> io::Result<RawFd> {
            let fd = self.as_raw_fd();
            let dup = unsafe { libc::dup(fd) };
            if dup < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(dup)
            }
        }

        /// Borrow the raw fd
        pub fn as_raw_fd(&self) -> RawFd {
            *self.fd
        }

        /// Create a std::fs::File from this handle (duplicates the fd)
        pub fn try_into_file(&self) -> io::Result<File> {
            let fd = self.as_raw_fd();
            // Duplicate then create File from it so we don't transfer ownership
            let dup = unsafe { libc::dup(fd) };
            if dup < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(unsafe { File::from_raw_fd(dup) })
            }
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // Only close when the Arc is the last owner
            if Arc::strong_count(&self.fd) == 1 {
                let fd = *self.fd;
                if fd >= 0 {
                    unsafe { libc::close(fd) };
                }
            }
        }
    }

    impl From<File> for OwnedHandle {
        fn from(f: File) -> Self {
            let fd = f.into_raw_fd();
            unsafe { OwnedHandle::from_raw_fd(fd) }
        }
    }

    impl From<std::os::unix::net::UnixStream> for OwnedHandle {
        fn from(s: std::os::unix::net::UnixStream) -> Self {
            let fd = s.into_raw_fd();
            unsafe { OwnedHandle::from_raw_fd(fd) }
        }
    }

    impl OwnedHandle {
        /// Read all bytes from the underlying fd by creating a File duplicate.
        pub fn read_to_end(&self, buf: &mut Vec<u8>) -> io::Result<usize> {
            let mut file = self.try_into_file()?;
            file.read_to_end(buf)
        }

        /// Write bytes to the underlying fd by creating a File duplicate.
        pub fn write_all(&self, buf: &[u8]) -> io::Result<()> {
            let mut file = self.try_into_file()?;
            file.write_all(buf)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::OwnedHandle;
        use std::os::unix::net::UnixStream;
        use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        // Run a closure in a thread and fail the test if it doesn't complete within `dur`.
        fn run_with_timeout<T, F>(dur: Duration, f: F) -> T
        where
            F: FnOnce() -> T + Send + 'static,
            T: Send + 'static,
        {
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                // Catch panics in the child and send them back to the tester thread.
                let res = catch_unwind(AssertUnwindSafe(f));
                let _ = tx.send(res);
            });

            match rx.recv_timeout(dur) {
                Ok(Ok(v)) => v,
                Ok(Err(payload)) => resume_unwind(payload),
                Err(mpsc::RecvTimeoutError::Timeout) => panic!("test timed out after {:?}", dur),
                Err(e) => panic!("recv error: {:?}", e),
            }
        }

        #[test]
        fn smoke_pair_read_write() {
            run_with_timeout(Duration::from_secs(2), || {
                let (a, b) = UnixStream::pair().unwrap();
                let a_handle = OwnedHandle::from(a);
                let b_handle = OwnedHandle::from(b);

                // write on a -> read on b
                a_handle.write_all(b"hello").unwrap();
                // Close the writer so read_to_end on the reader sees EOF and doesn't block
                drop(a_handle);
                let mut buf = Vec::new();
                // read some bytes (non-blocking behavior varies); try a short wait loop
                // We'll attempt to read; read_to_end may block until the peer closes, so use a small read via File
                b_handle.read_to_end(&mut buf).unwrap_or_default();
                assert!(buf.windows(5).any(|w| w == b"hello"));
            })
        }

        #[test]
        fn clone_and_drop() {
            run_with_timeout(Duration::from_secs(2), || {
                let (a, _b) = UnixStream::pair().unwrap();
                let h1 = OwnedHandle::from(a);
                let h2 = h1.try_clone().unwrap();
                assert_ne!(h1.as_raw_fd(), h2.as_raw_fd());
                // dropping both should not panic
                drop(h1);
                drop(h2);
            })
        }
    }
}

// Re-export for crate-wide use on unix
#[cfg(unix)]
pub use unix::OwnedHandle;
