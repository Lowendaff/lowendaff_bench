//! A temporary swap file (design cpu, §7.2). It is filled with zeros, gets the header that
//! `mkswap` would write, is turned on with `swapon`, and is turned off and removed again. The
//! two calls into the C library are the only `unsafe` code of the workspace (CP-07).

#![warn(missing_docs)]

use std::ffi::CString;
use std::fs;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

/// Bytes written at a time while the file is filled, between checks for cancellation.
const CHUNK: usize = 1 << 20;

/// The fewest pages a swap area may have, as `mkswap` requires.
const MIN_PAGES: u64 = 10;

/// A swap file that is on until [`SwapFile::remove`], or until it is dropped.
#[derive(Debug)]
pub struct SwapFile {
    path: PathBuf,
    c_path: CString,
    /// Whether the file is still on, so that a drop turns it off and removes it.
    active: bool,
}

impl SwapFile {
    /// Creates `path` with `size` bytes, rounded down to whole pages, and turns it on with
    /// the kernel's default priority, below the host's own swap. `cancelled` is asked between
    /// writes of 1 MiB; when it answers `true`, the file is removed and the error is of kind
    /// [`io::ErrorKind::Interrupted`]. On any error the file is removed.
    ///
    /// Blocks until the file is written, so async callers run it on a blocking thread.
    pub fn try_new(path: &Path, size: u64, cancelled: &dyn Fn() -> bool) -> io::Result<Self> {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::OpenOptionsExt;

        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let page_size = rustix::param::page_size();
        let pages = size / page_size as u64;
        let last_page = u32::try_from(pages.saturating_sub(1))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        if pages < MIN_PAGES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("a swap area needs at least {MIN_PAGES} pages"),
            ));
        }
        let file = File::options()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        // From here on the file is this call's own, and an error removes it.
        let outcome =
            fill(file, page_size, pages, last_page, cancelled).and_then(|()| swap_on(&c_path));
        if outcome.is_err() {
            let _ = fs::remove_file(path);
        }
        outcome.map(|()| Self {
            path: path.to_path_buf(),
            c_path,
            active: true,
        })
    }

    /// The file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Turns the swap off, which reads the pages still swapped out back into memory, and
    /// removes the file. When turning it off fails, the swap stays on and the file stays.
    pub fn remove(mut self) -> io::Result<()> {
        // The caller reports the outcome, so a failure is not tried again when `self` drops.
        self.active = false;
        self.turn_off_and_delete()
    }

    fn turn_off_and_delete(&self) -> io::Result<()> {
        swap_off(&self.c_path)?;
        fs::remove_file(&self.path)
    }
}

impl Drop for SwapFile {
    fn drop(&mut self) {
        if self.active {
            let _ = self.turn_off_and_delete();
        }
    }
}

/// The first page of a swap area whose last page is `last_page`: the header that
/// `mkswap --uuid clear` writes for an area without a label (`SWAPSPACE2`, version 1),
/// in the machine's byte order, with zeros elsewhere.
///
/// # Panics
///
/// When `page_size` is less than 4096 bytes, which no Linux architecture uses.
#[must_use]
pub fn header(page_size: usize, last_page: u32) -> Vec<u8> {
    assert!(page_size >= 4096, "pages are at least 4 KiB");
    let mut page = vec![0_u8; page_size];
    // After 1024 bytes for boot loaders: the version, the last page and the number of bad
    // pages; then the UUID and the label, which stay zero.
    page[1024..1028].copy_from_slice(&1_u32.to_ne_bytes());
    page[1028..1032].copy_from_slice(&last_page.to_ne_bytes());
    page[page_size - 10..].copy_from_slice(b"SWAPSPACE2");
    page
}

/// Writes the header page and zeros for the other pages, since a swap file must not have
/// holes, and makes them durable.
fn fill(
    mut file: File,
    page_size: usize,
    pages: u64,
    last_page: u32,
    cancelled: &dyn Fn() -> bool,
) -> io::Result<()> {
    file.write_all(&header(page_size, last_page))?;
    let zeros = vec![0_u8; CHUNK];
    let mut left = (pages - 1) * page_size as u64;
    while left > 0 {
        if cancelled() {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let length = usize::try_from(left).map_or(CHUNK, |left| left.min(CHUNK));
        file.write_all(&zeros[..length])?;
        left -= length as u64;
    }
    file.sync_all()
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn swap_on(path: &CString) -> io::Result<()> {
    // SAFETY: `path` is a NUL-terminated string that lives for the whole call, and swapon
    // only reads it. The flags 0 ask for the default priority.
    let status = unsafe { libc::swapon(path.as_ptr(), 0) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn swap_off(path: &CString) -> io::Result<()> {
    // SAFETY: `path` is a NUL-terminated string that lives for the whole call, and swapoff
    // only reads it.
    let status = unsafe { libc::swapoff(path.as_ptr()) };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn swap_on(_path: &CString) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(not(target_os = "linux"))]
fn swap_off(_path: &CString) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}
