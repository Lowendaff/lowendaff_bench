//! The glibc reader, on synthetic hosts laid out like real distributions.

mod common;

use bench_api::GlibcVersion;
use bench_api::Libc;
use bench_host::Root;
use bench_host::glibc;

/// How glibc describes itself inside `libc.so.6`, between binary data.
fn library(banner: &str) -> Vec<u8> {
    let mut bytes = b"\x7fELF\x02\x01\x01\0\0\0rodata\0".to_vec();
    bytes.extend_from_slice(banner.as_bytes());
    bytes.extend_from_slice(b"\nCopyright (C) 2025 Free Software Foundation, Inc.\n\0");
    bytes
}

fn glibc(major: u32, minor: u32) -> Libc {
    Libc::Glibc(GlibcVersion { major, minor })
}

#[test]
fn debian_with_a_merged_usr_reads_through_relative_links() {
    let host = common::host((
        &[("/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2", "ld")],
        &[],
    ));
    common::link(&host, "/lib64", "usr/lib64");
    common::link(
        &host,
        "/usr/lib64/ld-linux-x86-64.so.2",
        "../lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
    );
    common::add_bytes(
        &host,
        "/usr/lib/x86_64-linux-gnu/libc.so.6",
        &library("GNU C Library (Debian GLIBC 2.41-12) stable release version 2.41."),
    );
    assert_eq!(glibc::read(&Root::new(host.path()), "x86_64"), glibc(2, 41));
}

#[test]
fn el8_reads_through_an_absolute_link_and_a_versioned_library() {
    let host = common::host((&[("/usr/lib64/ld-2.28.so", "ld")], &[]));
    common::link(&host, "/lib", "usr/lib");
    common::link(
        &host,
        "/usr/lib/ld-linux-aarch64.so.1",
        "/usr/lib64/ld-2.28.so",
    );
    common::link(&host, "/usr/lib64/libc.so.6", "libc-2.28.so");
    common::add_bytes(
        &host,
        "/usr/lib64/libc-2.28.so",
        &library("GNU C Library (GNU libc) stable release version 2.28."),
    );
    assert_eq!(
        glibc::read(&Root::new(host.path()), "aarch64"),
        glibc(2, 28)
    );
}

#[test]
fn rhel_on_aarch64_reads_the_loader_that_lies_apart_from_libc() {
    // Rocky Linux 9 for aarch64: the loader is a file in /usr/lib, libc.so.6 is in /usr/lib64.
    let host = common::host((&[], &[]));
    common::link(&host, "/lib", "usr/lib");
    common::link(&host, "/lib64", "usr/lib64");
    common::add_bytes(
        &host,
        "/usr/lib/ld-linux-aarch64.so.1",
        &library("ld.so (GNU libc) stable release version 2.34."),
    );
    common::add_bytes(
        &host,
        "/usr/lib64/libc.so.6",
        &library("GNU C Library (GNU libc) stable release version 2.34."),
    );
    let root = Root::new(host.path());
    assert_eq!(glibc::read(&root, "aarch64"), glibc(2, 34));

    std::fs::remove_file(host.path().join("usr/lib64/libc.so.6")).expect("remove libc");
    assert_eq!(
        glibc::read(&root, "aarch64"),
        glibc(2, 34),
        "the loader alone tells the version"
    );
}

#[test]
fn the_loaders_own_version_comes_before_the_library_beside_it() {
    let host = common::host((&[], &[]));
    common::add_bytes(
        &host,
        "/lib64/ld-linux-x86-64.so.2",
        &library("ld.so (GNU libc) stable release version 2.39."),
    );
    common::add_bytes(
        &host,
        "/lib64/libc.so.6",
        &library("GNU C Library (GNU libc) stable release version 2.17."),
    );
    assert_eq!(glibc::read(&Root::new(host.path()), "x86_64"), glibc(2, 39));
}

#[test]
fn a_development_version_keeps_its_major_and_minor_numbers() {
    let host = common::host((&[("/lib/ld-linux-aarch64.so.1", "ld")], &[]));
    common::add_bytes(
        &host,
        "/lib/libc.so.6",
        &library("GNU C Library (GNU libc) development release version 2.42.9000."),
    );
    assert_eq!(
        glibc::read(&Root::new(host.path()), "aarch64"),
        glibc(2, 42)
    );
}

#[test]
fn a_system_without_the_glibc_loader_has_no_glibc() {
    let alpine = common::host((&[("/lib/ld-musl-aarch64.so.1", "musl")], &[]));
    let root = Root::new(alpine.path());
    assert_eq!(glibc::read(&root, "aarch64"), Libc::NoGlibc);
    assert_eq!(glibc::read(&root, "x86_64"), Libc::NoGlibc);
}

#[test]
fn a_loader_whose_library_tells_no_version_leaves_the_version_unknown() {
    let host = common::host((&[("/lib64/ld-linux-x86-64.so.2", "ld")], &[]));
    let root = Root::new(host.path());
    assert_eq!(
        glibc::read(&root, "x86_64"),
        Libc::GlibcUnknown,
        "libc.so.6 is missing"
    );

    common::add_bytes(&host, "/lib64/libc.so.6", &library("GNU C Library"));
    assert_eq!(glibc::read(&root, "x86_64"), Libc::GlibcUnknown);

    common::add_bytes(&host, "/lib64/libc.so.6", &library("release version 2."));
    assert_eq!(
        glibc::read(&root, "x86_64"),
        Libc::GlibcUnknown,
        "a version without a minor number"
    );
}

#[test]
fn a_loop_of_links_counts_as_no_loader() {
    let host = common::host((&[], &[]));
    common::link(&host, "/lib64", "lib");
    common::link(&host, "/lib", "lib64");
    assert_eq!(
        glibc::read(&Root::new(host.path()), "x86_64"),
        Libc::NoGlibc
    );
}

#[test]
fn other_architectures_have_no_known_loader() {
    let host = common::host((&[("/lib/ld-linux-riscv64-lp64d.so.1", "ld")], &[]));
    assert_eq!(
        glibc::read(&Root::new(host.path()), "riscv64"),
        Libc::GlibcUnknown
    );
}
