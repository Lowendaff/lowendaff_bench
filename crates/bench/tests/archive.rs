//! Tools that come as archives (`.tar.gz`, flat `.tar.gz` with sibling symlinks, `.tar.xz`, and
//! `.zip`): checked before unpacking, unpacked cleanly, and refused when an entry could land
//! outside the tool's directory or fails integrity checks.

use std::fmt::Write as _;
use std::io;
use std::io::Write as _;
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
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    assets: &[],
};

const PRIME95: ToolSpec = ToolSpec {
    name: "prime95",
    version: "30.19b20",
    all_arches: false,
    raw_sockets: false,
    form: ToolForm::TarGzFlat {
        executable: "mprime",
    },
    assets: &[],
};

const YCRUNCHER: ToolSpec = ToolSpec {
    name: "ycruncher",
    version: "0.8.7.9547",
    all_arches: false,
    raw_sockets: false,
    form: ToolForm::TarXz {
        executable: "y-cruncher",
    },
    assets: &[],
};

const PASSMARK: ToolSpec = ToolSpec {
    name: "passmark",
    version: "11.0.1004",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Zip {
        x86_64: "PerformanceTest_Linux_x86-64",
        aarch64: "PerformanceTest_Linux_ARM64",
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

/// Builds an uncompressed tar byte stream of `entries`.
fn tar_bytes(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
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
    builder.into_inner().expect("tar stream")
}

/// A gzip-compressed tar archive of `entries`, built as tar itself builds archives.
fn archive(entries: &[Entry<'_>]) -> Vec<u8> {
    let raw = tar_bytes(entries);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&raw).expect("gzip write");
    gz.finish().expect("gzip finish")
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

/// The pin of exactly these bytes on `arch`.
fn asset_arch(bytes: &[u8], arch: &'static str) -> ToolAsset {
    use sha2::Digest;

    let mut digest = String::new();
    for byte in sha2::Sha256::digest(bytes) {
        write!(digest, "{byte:02x}").expect("write to a string");
    }
    ToolAsset {
        arch,
        url: "https://tools.example/archive",
        sha256: Box::leak(digest.into_boxed_str()),
        size: bytes.len() as u64,
    }
}

fn asset(bytes: &[u8]) -> ToolAsset {
    asset_arch(bytes, "x86_64")
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

    assert!(err.starts_with("SHA-256 mismatch: "), "{err}");
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
        assert!(
            err.starts_with("the archive holds an unsupported entry: "),
            "{case}: {err}"
        );
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

    assert_eq!(err, "the archive holds no geekbench7");
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[tokio::test]
async fn a_damaged_archive_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"not a gzip stream".to_vec();
    let err = provision::install(&bytes[..], &GEEKBENCH, &asset(&bytes), dir.path())
        .await
        .unwrap_err();

    assert!(err.starts_with("damaged archive: "), "{err}");
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[tokio::test]
async fn flat_tar_gz_accepts_sibling_symlinks_and_rejects_escaping_symlinks() {
    use std::os::unix::fs::PermissionsExt;

    let good = {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut h1 = Header::new_gnu();
        h1.set_mode(0o644);
        h1.set_size(12);
        builder
            .append_data(&mut h1, "mprime", &b"#!/bin/sh\n\n\n"[..])
            .unwrap();
        let mut h2 = Header::new_gnu();
        h2.set_mode(0o755);
        h2.set_size(4);
        builder
            .append_data(&mut h2, "libgmp.so.10.4.1", &b"elf!"[..])
            .unwrap();
        let mut h3 = Header::new_gnu();
        h3.set_entry_type(EntryType::Symlink);
        h3.set_size(0);
        builder
            .append_link(&mut h3, "libgmp.so.10", "libgmp.so.10.4.1")
            .unwrap();
        finish(builder)
    };
    let dir = tempfile::tempdir().unwrap();
    let path = provision::install(&good[..], &PRIME95, &asset(&good), dir.path())
        .await
        .unwrap();
    assert_eq!(path, dir.path().join("prime95/mprime"));
    assert_ne!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
        0
    );
    assert_eq!(
        std::fs::read(dir.path().join("prime95/libgmp.so.10")).unwrap(),
        b"elf!"
    );

    let escaping = {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut h = Header::new_gnu();
        h.set_entry_type(EntryType::Symlink);
        h.set_size(0);
        builder
            .append_link(&mut h, "libgmp.so.10", "../outside")
            .unwrap();
        finish(builder)
    };
    let dir2 = tempfile::tempdir().unwrap();
    let err = provision::install(&escaping[..], &PRIME95, &asset(&escaping), dir2.path())
        .await
        .unwrap_err();
    assert!(
        err.starts_with("the archive holds an unsupported entry: "),
        "{err}"
    );
}

#[tokio::test]
async fn tar_xz_unpacks_and_removes_temporary_tar() {
    use std::os::unix::fs::PermissionsExt;

    let raw_tar = tar_bytes(&[
        file("y-cruncher v0.8.7/", 0o755, b""),
        file("y-cruncher v0.8.7/y-cruncher", 0o644, b"#!/bin/sh\n"),
        file("y-cruncher v0.8.7/Binaries/", 0o755, b""),
        file("y-cruncher v0.8.7/Binaries/tune.bin", 0o644, b"avx2"),
    ]);
    let mut xz = Vec::new();
    lzma_rs::xz_compress(&mut io::Cursor::new(&raw_tar), &mut xz).expect("xz compress");

    let dir = tempfile::tempdir().unwrap();
    let path = provision::install(&xz[..], &YCRUNCHER, &asset(&xz), dir.path())
        .await
        .unwrap();
    assert_eq!(path, dir.path().join("ycruncher/y-cruncher"));
    assert_eq!(names(dir.path()), ["ycruncher"]);
    assert_ne!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
        0
    );
    assert_eq!(
        std::fs::read(dir.path().join("ycruncher/Binaries/tune.bin")).unwrap(),
        b"avx2"
    );
}

/// Builds a minimal valid 32-bit ZIP archive with Deflated (`method = 8`) entries.
fn build_zip(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for &(name, content, mode) in entries {
        let local_offset = u32::try_from(out.len()).expect("zip offset fits in u32");
        let mut crc_hasher = crc32fast::Hasher::new();
        crc_hasher.update(content);
        let crc = crc_hasher.finalize();
        let (method, payload) = if content.is_empty() {
            (0_u16, Vec::new())
        } else {
            let mut enc =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            enc.write_all(content).expect("deflate write");
            (8_u16, enc.finish().expect("deflate finish"))
        };
        let comp_len = u32::try_from(payload.len()).expect("compressed length fits in u32");
        let uncomp_len = u32::try_from(content.len()).expect("uncompressed length fits in u32");
        let name_len = u16::try_from(name.len()).expect("entry name fits in u16");

        // Local file header
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        out.extend_from_slice(&20_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&comp_len.to_le_bytes());
        out.extend_from_slice(&uncomp_len.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&payload);

        // Central directory header
        let ext_attr = mode << 16;
        central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        central.extend_from_slice(&0x0314_u16.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&comp_len.to_le_bytes());
        central.extend_from_slice(&uncomp_len.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&ext_attr.to_le_bytes());
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).expect("central directory offset fits in u32");
    let cd_size = u32::try_from(central.len()).expect("central directory size fits in u32");
    let count = u16::try_from(entries.len()).expect("entry count fits in u16");
    out.extend_from_slice(&central);
    // EOCD
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

#[tokio::test]
async fn zip_archive_unpacks_per_architecture_and_verifies_crc() {
    use std::os::unix::fs::PermissionsExt;

    let zip_arm = build_zip(&[
        ("PerformanceTest/", b"", 0o040_755),
        (
            "PerformanceTest/PerformanceTest_Linux_ARM64",
            b"#!/bin/sh\necho ok\n",
            0o100_644,
        ),
        ("PerformanceTest/readme.txt", b"hello", 0o100_644),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let path = provision::install(
        &zip_arm[..],
        &PASSMARK,
        &asset_arch(&zip_arm, "aarch64"),
        dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        path,
        dir.path().join("passmark/PerformanceTest_Linux_ARM64")
    );
    assert_ne!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
        0
    );
    assert_eq!(
        std::fs::read(dir.path().join("passmark/readme.txt")).unwrap(),
        b"hello"
    );
}
