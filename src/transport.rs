//! The byte stream under the herdr socket API, per platform.
//!
//! herdr speaks the same newline-delimited JSON over a Unix domain socket on
//! Linux and macOS and over a named pipe on Windows, and injects the endpoint
//! as `HERDR_SOCKET_PATH` either way. Only the connecting differs, so it is
//! isolated here and the protocol code stays platform-neutral.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A connected stream to the herdr socket.
pub struct Stream(imp::Inner);

impl Stream {
    pub fn connect(path: &Path) -> std::io::Result<Self> {
        imp::connect(path).map(Stream)
    }

    /// A second handle on the same connection, so a request can be written
    /// while the response is read.
    pub fn try_clone(&self) -> std::io::Result<Self> {
        imp::try_clone(&self.0).map(Stream)
    }

    /// Bounds a blocking read. `None` waits indefinitely, which is what the
    /// event stream wants.
    ///
    /// On Windows this is a no-op: the standard library exposes no timeout for
    /// named pipe handles. A request there blocks until herdr answers.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        imp::set_read_timeout(&self.0, timeout)
    }

    /// Bounds a blocking write. A no-op on Windows, for the same reason.
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        imp::set_write_timeout(&self.0, timeout)
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Whether a path is worth trying, used to fail with a clear message rather
/// than a confusing connect error.
///
/// Only meaningful for a socket file; a named pipe does not answer existence
/// checks the same way, so Windows always says yes and lets connect decide.
pub fn looks_present(path: &Path) -> bool {
    imp::looks_present(path)
}

/// Where herdr puts its socket when `HERDR_SOCKET_PATH` is absent.
///
/// The variable is injected into every plugin command, so this only matters
/// when running the binary by hand.
pub fn default_socket_path() -> Option<PathBuf> {
    imp::default_socket_path()
}

/// The user's home directory: `HOME` on Unix, `USERPROFILE` on Windows.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(unix)]
mod imp {
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    pub type Inner = UnixStream;

    pub fn connect(path: &Path) -> std::io::Result<Inner> {
        UnixStream::connect(path)
    }

    pub fn try_clone(stream: &Inner) -> std::io::Result<Inner> {
        stream.try_clone()
    }

    pub fn set_read_timeout(stream: &Inner, timeout: Option<Duration>) -> std::io::Result<()> {
        stream.set_read_timeout(timeout)
    }

    pub fn set_write_timeout(stream: &Inner, timeout: Option<Duration>) -> std::io::Result<()> {
        stream.set_write_timeout(timeout)
    }

    pub fn looks_present(path: &Path) -> bool {
        path.exists()
    }

    pub fn default_socket_path() -> Option<PathBuf> {
        super::home_dir().map(|home| home.join(".config/herdr/herdr.sock"))
    }
}

#[cfg(windows)]
mod imp {
    use std::fs::{File, OpenOptions};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    pub type Inner = File;

    /// A named pipe client is opened like a file, read-write.
    pub fn connect(path: &Path) -> std::io::Result<Inner> {
        OpenOptions::new().read(true).write(true).open(path)
    }

    pub fn try_clone(stream: &Inner) -> std::io::Result<Inner> {
        stream.try_clone()
    }

    /// No standard-library timeout exists for a named pipe handle.
    pub fn set_read_timeout(_stream: &Inner, _timeout: Option<Duration>) -> std::io::Result<()> {
        Ok(())
    }

    pub fn set_write_timeout(_stream: &Inner, _timeout: Option<Duration>) -> std::io::Result<()> {
        Ok(())
    }

    /// `\\.\pipe\...` does not behave like a file for existence checks, so let
    /// the connect attempt be the test.
    pub fn looks_present(_path: &Path) -> bool {
        true
    }

    pub fn default_socket_path() -> Option<PathBuf> {
        Some(PathBuf::from(r"\\.\pipe\herdr"))
    }
}
