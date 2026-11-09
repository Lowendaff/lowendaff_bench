//! Tools that come as archives (design cpu, §6.3): checked before unpacking, unpacked without
//! their top-level directory, and refused when an entry could land outside the tool's
//! directory.

use std::fmt::Write;
use std::io;
use std::path::Path;

use bench::provision;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use tar::EntryType;
use tar::Header;

const GEEKBENCH: ToolSpec = ToolSpec {
    name: "geekbench",
    version: "7.1.0",
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    assets: &[],
};

const TOP: &str = "Geekbench-7.1.0-Linux";

/// An archive entry: a directory when its path ends in `/`, else a file with these bytes.
struct Entry<'a> {
    path: &'a str,
    mode: u32,
    contents: &'a [u8],
}

const fn file<'a>(path: &'a str, mode: u32, contents: &'a [u8]) -> Entry<'a> {
    Entry {
        path,
        mode,
        contents,
    }
}

/// A gzip-compressed tar archive of `entries`, built as tar itself builds archives.
fn archive(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    for entry in entries {
        let mut header = Header::new_gnu();
        header.set_mode(entry.mode);
        if let Some(dir) = entry.path.strip_suffix('/') {
            header.set_entry_type(EntryType::Directory);
            header.set_size(0);
            builder
                .append_data(&mut header, dir, io::empty())
                .expect("directory entry");
        } else {
            header.set_size(entry.contents.len() as u64);
            builder
                .append_data(&mut header, entry.path, entry.contents)
                .expect("file entry");
        }
    }
    finish(builder)
}

/// An archive with one file whose path is written into the header as is, which tar's builder
/// refuses for paths with `..` or a leading `/`.
fn raw_archive(path: &str) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    ));
    let mut header = Header::new_old();
    header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o644);
    header.set_size(4);
    header.set_cksum();
    builder.append(&header, &b"evil"[..]).expect("raw entry");
    finish(builder)
}

fn finish(builder: tar::Builder<flate2::write::GzEncoder<Vec<u8>>>) -> Vec<u8> {
    builder
        .into_inner()
        .expect("tar stream")
        .finish()
        .expect("gzip stream")
}

/// The pin of exactly these bytes.
fn asset(bytes: &[u8]) -> ToolAsset {
    use sha2::Digest;

    let mut digest = String::new();
    for byte in sha2::Sha256::digest(bytes) {
        write!(digest, "{byte:02x}").expect("write to a string");
    }
    ToolAsset {
        arch: "x86_64",
        url: "https://tools.example/geekbench.tar.gz",
        sha256: Box::leak(digest.into_boxed_str()),
        size: bytes.len() as u64,
    }
}

/// Names in a directory, sorted.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn good_archive() -> Vec<u8> {
    archive(&[
        file(&format!("{TOP}/"), 0o755, b""),
        file(&format!("{TOP}/geekbench7"), 0o755, b"#!/bin/sh\n"),
        file(&format!("{TOP}/geekbench.plxr"), 0o644, b"workloads"),
        file(&format!("{TOP}/data/"), 0o755, b""),
        file(&format!("{TOP}/data/model.bin"), 0o644, b"weights"),
    ])
}

#[tokio::test]
async fn an_archive_is_unpacked_without_its_top_directory_and_keeps_executable_bits() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let bytes = good_archive();
    let path = provision::install(&bytes[..], &GEEKBENCH, &asset(&bytes), dir.path())
        .await
        .unwrap();

    let tool = dir.path().join("geekbench");
    assert_eq!(path, tool.join("geekbench7"));
    assert_eq!(
        names(dir.path()),
        ["geekbench"],
        "no archive or staging left"
    );
    assert_eq!(names(&tool), ["data", "geekbench.plxr", "geekbench7"]);
    assert_eq!(
        std::fs::read(tool.join("data/model.bin")).unwrap(),
        b"weights"
    );
    let mode = |name: &str| {
        std::fs::metadata(tool.join(name))
            .unwrap()
            .permissions()
            .mode()
    };
    assert_ne!(mode("geekbench7") & 0o111, 0);
    assert_eq!(mode("geekbench.plxr") & 0o111, 0);
}

#[tokio::test]
async fn an_archive_that_fails_its_check_is_not_unpacked() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = good_archive();
    let wrong = ToolAsset {
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        ..asset(&bytes)
    };
    let err = provision::install(&bytes[..], &GEEKBENCH, &wrong, dir.path())
        .await
        .unwrap_err();

    assert!(err.starts_with("SHA-256 不符："), "{err}");
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[tokio::test]
async fn archives_with_entries_that_could_escape_are_refused_and_leave_nothing() {
    let link = {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut header = Header::new_gnu();
        header.set_entry_type(EntryType::Symlink);
        header.set_size(0);
        builder
            .append_link(&mut header, format!("{TOP}/geekbench7"), "/bin/sh")
            .expect("link entry");
        finish(builder)
    };
    let cases = [
        ("a link", link),
        ("a path with ..", raw_archive(&format!("{TOP}/../evil"))),
        ("an absolute path", raw_archive("/tmp/evil")),
        (
            "two top-level directories",
            archive(&[
                file(&format!("{TOP}/geekbench7"), 0o755, b"#!/bin/sh\n"),
                file("other/geekbench7", 0o755, b"#!/bin/sh\n"),
            ]),
        ),
        (
            "a file at the top level",
            archive(&[file("geekbench7", 0o755, b"#!/bin/sh\n")]),
        ),
    ];
    for (case, bytes) in cases {
        let dir = tempfile::tempdir().unwrap();
        let err = provision::install(&bytes[..], &GEEKBENCH, &asset(&bytes), dir.path())
            .await
            .unwrap_err();
        assert!(err.starts_with("压缩包里有不支持的条目："), "{case}: {err}");
        assert!(
            names(dir.path()).is_empty(),
            "{case}: {:?}",
            names(dir.path())
        );
    }
}

#[tokio::test]
async fn an_archive_without_the_executable_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = archive(&[file(&format!("{TOP}/geekbench.plxr"), 0o644, b"workloads")]);
    let err = provision::install(&bytes[..], &GEEKBENCH, &asset(&bytes), dir.path())
        .await
        .unwrap_err();

    assert_eq!(err, "压缩包里没有 geekbench7");
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[tokio::test]
async fn a_damaged_archive_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"not a gzip stream".to_vec();
    let err = provision::install(&bytes[..], &GEEKBENCH, &asset(&bytes), dir.path())
        .await
        .unwrap_err();

    assert!(err.starts_with("压缩包损坏："), "{err}");
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}
