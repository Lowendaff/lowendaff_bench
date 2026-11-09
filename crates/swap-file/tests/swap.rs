//! The swap header against util-linux's `mkswap`, and the clean-up of a swap file that could
//! not be turned on. Turning swap on needs root on Linux; the CI step "Swap round trip" covers
//! that with the example `roundtrip`.

use std::io;
use std::path::PathBuf;

use swap_file::SwapFile;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

#[test]
fn the_header_is_what_mkswap_writes() {
    assert_eq!(
        swap_file::header(4096, 255),
        fixture("mkswap-1MiB-page-4096.bin")
    );
    assert_eq!(
        swap_file::header(65536, 15),
        fixture("mkswap-1MiB-page-65536.bin")
    );
}

#[test]
fn a_swap_file_leaves_nothing_behind_whether_or_not_it_could_be_turned_on() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("swapfile");
    match SwapFile::try_new(&path, 4 << 20, &|| false) {
        // As root on Linux: turned on, so turn it off again.
        Ok(swap) => {
            assert_eq!(swap.path(), path);
            swap.remove().expect("turned off and removed");
        }
        // Elsewhere, swapon fails after the file was written: without root on Linux, on
        // other systems, or as root where the filesystem has no swap files (tmpfs).
        Err(err) => assert!(
            matches!(
                err.kind(),
                io::ErrorKind::PermissionDenied
                    | io::ErrorKind::Unsupported
                    | io::ErrorKind::InvalidInput
            ),
            "{err}"
        ),
    }
    assert!(!path.exists(), "the file is gone");
}

#[test]
fn a_cancelled_fill_stops_and_removes_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("swapfile");
    let err = SwapFile::try_new(&path, 64 << 20, &|| true).expect_err("cancelled");
    assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    assert!(!path.exists());
}

#[test]
fn a_swap_area_needs_ten_pages_and_a_new_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("swapfile");
    let five_pages = 5 * rustix::param::page_size() as u64;
    let err = SwapFile::try_new(&path, five_pages, &|| false).expect_err("too small");
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    assert!(!path.exists(), "nothing is created for a refused size");

    std::fs::write(&path, b"someone else's file").expect("write");
    let err = SwapFile::try_new(&path, 4 << 20, &|| false).expect_err("exists");
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        std::fs::read(&path).expect("read"),
        b"someone else's file",
        "a file that was there before is left alone"
    );
}
