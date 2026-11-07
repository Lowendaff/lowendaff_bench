//! Helpers shared by the bench-host tests: captured fixtures and small synthetic hosts.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::path::PathBuf;

use bench_host::Root;
use bench_host::platform::Cpuid;
use bench_host::storage::Space;
use tempfile::TempDir;

/// The directory of a captured host (tests/fixtures/README.md).
pub fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The files of a captured host.
pub fn fixture(name: &str) -> Root {
    Root::new(fixture_dir(name).join("root"))
}

/// The CPUID recorded for a captured host; `None` for captures without CPUID.
pub fn fixture_cpuid(name: &str) -> Option<Cpuid> {
    let text = fs::read_to_string(fixture_dir(name).join("cpuid")).ok()?;
    Some(cpuid(text.trim()))
}

/// `signature` is the hypervisor signature, or `-` when the hypervisor bit is clear.
pub fn cpuid(signature: &str) -> Cpuid {
    Cpuid {
        hypervisor: (signature != "-").then(|| signature.to_owned()),
    }
}

/// The output of a host tool recorded at capture time, such as `df` or `free`.
pub fn truth(name: &str, tool: &str) -> String {
    fs::read_to_string(fixture_dir(name).join("truth").join(format!("{tool}.txt")))
        .expect("truth file")
}

/// Filesystem sizes as `df -B1 --output=target,size,used,avail` printed them at capture
/// time, standing in for statvfs.
pub fn df(name: &str) -> impl Fn(&str) -> Option<Space> {
    let text = truth(name, "df");
    move |mount_point: &str| {
        text.lines().skip(1).find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [target, size, used, available] = fields.as_slice() else {
                return None;
            };
            (*target == mount_point).then(|| Space {
                total: size.parse().expect("size"),
                used: used.parse().expect("used"),
                available: available.parse().expect("available"),
            })
        })
    }
}

/// Files (host path, contents) and empty directories of a synthetic host.
pub type Layout = (
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
);

/// A synthetic host laid out like `/`.
pub fn host((files, dirs): Layout) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    add(&dir, (files, dirs));
    dir
}

/// Adds files and directories to a synthetic host.
pub fn add(dir: &TempDir, (files, dirs): Layout) {
    for (path, contents) in files {
        let path = dir.path().join(path.trim_start_matches('/'));
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, contents).expect("write file");
    }
    for path in dirs {
        fs::create_dir_all(dir.path().join(path.trim_start_matches('/'))).expect("create dir");
    }
}

/// Adds a symbolic link to a synthetic host.
pub fn link(dir: &TempDir, path: &str, target: &str) {
    let path = dir.path().join(path.trim_start_matches('/'));
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    symlink(target, path).expect("symlink");
}

/// Adds a block device to `/sys/class/block`: `major:minor`, size in 512-byte sectors,
/// whether it is a partition, and the devices below it (LVM and RAID).
pub fn block(dir: &TempDir, name: &str, dev: &str, sectors: u64, partition: bool, slaves: &[&str]) {
    let base = dir.path().join("sys/class/block").join(name);
    fs::create_dir_all(base.join("queue")).expect("block dir");
    fs::write(base.join("dev"), format!("{dev}\n")).expect("dev");
    fs::write(base.join("size"), format!("{sectors}\n")).expect("size");
    fs::write(base.join("queue/rotational"), "0\n").expect("rotational");
    if partition {
        fs::write(base.join("partition"), "1\n").expect("partition");
    }
    if !slaves.is_empty() {
        fs::create_dir_all(base.join("slaves")).expect("slaves");
        for slave in slaves {
            fs::write(base.join("slaves").join(slave), "").expect("slave");
        }
    }
}
