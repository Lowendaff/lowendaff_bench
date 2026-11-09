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

/// Adds a file with binary contents to a synthetic host.
pub fn add_bytes(dir: &TempDir, path: &str, contents: &[u8]) {
    let path = dir.path().join(path.trim_start_matches('/'));
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, contents).expect("write file");
}

/// Adds a sparse file, such as `/dev/mem`, with `chunks` written at their offsets.
pub fn add_sparse(dir: &TempDir, path: &str, chunks: &[(u64, &[u8])]) {
    use std::io::Seek;
    use std::io::Write;

    let path = dir.path().join(path.trim_start_matches('/'));
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    let mut file = fs::File::create(path).expect("create sparse file");
    for (offset, bytes) in chunks {
        file.seek(std::io::SeekFrom::Start(*offset)).expect("seek");
        file.write_all(bytes).expect("write chunk");
    }
}

/// A captured file's bytes.
pub fn fixture_bytes(name: &str, path: &str) -> Vec<u8> {
    fs::read(
        fixture_dir(name)
            .join("root")
            .join(path.trim_start_matches('/')),
    )
    .expect("fixture file")
}

/// An SMBIOS structure: its type, handle, the fields after the 4-byte header, and its strings.
pub fn structure(kind: u8, handle: u16, fields: &[u8], strings: &[&str]) -> Vec<u8> {
    let mut bytes = vec![
        kind,
        u8::try_from(fields.len() + 4).expect("structure length"),
    ];
    bytes.extend(handle.to_le_bytes());
    bytes.extend(fields);
    for string in strings {
        bytes.extend(string.as_bytes());
        bytes.push(0);
    }
    if strings.is_empty() {
        bytes.push(0);
    }
    bytes.push(0);
    bytes
}

/// The fields of a memory device (type 17) that the reader uses; strings left empty are absent.
#[derive(Clone, Copy)]
pub struct Device<'a> {
    pub array: u16,
    pub size: u16,
    pub extended_size: u32,
    pub locator: &'a str,
    pub bank: &'a str,
    pub kind: u8,
    pub detail: u16,
    pub speed: u16,
    pub manufacturer: &'a str,
    pub part: &'a str,
    pub configured_speed: u16,
    pub extended_speed: u32,
    pub extended_configured_speed: u32,
}

impl Default for Device<'_> {
    fn default() -> Self {
        Self {
            array: 0x10,
            size: 0,
            extended_size: 0,
            locator: "",
            bank: "",
            kind: 0x02,
            detail: 0x0004,
            speed: 0,
            manufacturer: "",
            part: "",
            configured_speed: 0,
            extended_speed: 0,
            extended_configured_speed: 0,
        }
    }
}

/// A memory device of SMBIOS 3.3 and later, which have the extended speeds.
pub fn memory_device(handle: u16, device: Device<'_>) -> Vec<u8> {
    let mut strings = Vec::new();
    let locator = string_number(&mut strings, device.locator);
    let bank = string_number(&mut strings, device.bank);
    let manufacturer = string_number(&mut strings, device.manufacturer);
    let part = string_number(&mut strings, device.part);
    let mut fields = vec![0_u8; 0x5C - 4];
    let mut put = |offset: usize, bytes: &[u8]| {
        fields[offset - 4..offset - 4 + bytes.len()].copy_from_slice(bytes);
    };
    put(0x04, &device.array.to_le_bytes());
    put(0x06, &0xFFFE_u16.to_le_bytes());
    put(0x08, &72_u16.to_le_bytes());
    put(0x0A, &64_u16.to_le_bytes());
    put(0x0C, &device.size.to_le_bytes());
    put(0x0E, &[0x09]);
    put(0x10, &[locator, bank]);
    put(0x12, &[device.kind]);
    put(0x13, &device.detail.to_le_bytes());
    put(0x15, &device.speed.to_le_bytes());
    put(0x17, &[manufacturer]);
    put(0x1A, &[part]);
    put(0x1C, &device.extended_size.to_le_bytes());
    put(0x20, &device.configured_speed.to_le_bytes());
    put(0x54, &device.extended_speed.to_le_bytes());
    put(0x58, &device.extended_configured_speed.to_le_bytes());
    structure(17, handle, &fields, &strings)
}

/// The number a structure's field stores for `text`, adding it to the strings; 0 when empty.
fn string_number<'a>(strings: &mut Vec<&'a str>, text: &'a str) -> u8 {
    if text.is_empty() {
        return 0;
    }
    strings.push(text);
    u8::try_from(strings.len()).expect("string number")
}

/// A physical memory array (type 16): its use (3 is system memory, 4 video memory) and its
/// error correction (3 none, 4 parity, 5 single-bit, 6 multi-bit, 7 CRC).
pub fn memory_array(handle: u16, usage: u8, correction: u8) -> Vec<u8> {
    let mut fields = vec![0x03, usage, correction];
    fields.extend(0x0100_0000_u32.to_le_bytes());
    fields.extend(0xFFFE_u16.to_le_bytes());
    fields.extend(4_u16.to_le_bytes());
    fields.extend(0_u64.to_le_bytes());
    structure(16, handle, &fields, &[])
}

/// IPMI device information (type 38): the interface (1 KCS, 2 SMIC, 3 BT, 4 SSIF) and the
/// specification revision in BCD.
pub fn ipmi_device(handle: u16, interface: u8, revision: u8) -> Vec<u8> {
    let mut fields = vec![interface, revision, 0x20, 0xFF];
    fields.extend(0xCA2_u64.to_le_bytes());
    fields.extend([0, 0]);
    structure(38, handle, &fields, &[])
}

/// The end-of-table structure (type 127).
pub fn end_of_table(handle: u16) -> Vec<u8> {
    structure(127, handle, &[], &[])
}

/// An SMBIOS 3.x entry point (`_SM3_`) for a table of at most `maximum` bytes.
pub fn entry_point_3(address: u64, maximum: u32) -> Vec<u8> {
    let mut entry = b"_SM3_".to_vec();
    entry.extend([0, 0x18, 3, 2, 1, 1, 0]);
    entry.extend(maximum.to_le_bytes());
    entry.extend(address.to_le_bytes());
    entry[5] = checksum(&entry);
    // Readers take 32 bytes at an entry point, as dmidecode does.
    entry.resize(32, 0);
    entry
}

/// An SMBIOS 2.x entry point (`_SM_` with its `_DMI_` part) for a table of `length` bytes.
pub fn entry_point_2(address: u32, length: u16, count: u16) -> Vec<u8> {
    let mut entry = b"_SM_".to_vec();
    entry.extend([0, 0x1F, 3, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
    entry.extend(b"_DMI_");
    entry.push(0);
    entry.extend(length.to_le_bytes());
    entry.extend(address.to_le_bytes());
    entry.extend(count.to_le_bytes());
    entry.push(0x32);
    entry[0x15] = checksum(&entry[0x10..]);
    entry[4] = checksum(&entry);
    entry.resize(32, 0);
    entry
}

fn checksum(bytes: &[u8]) -> u8 {
    0_u8.wrapping_sub(bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)))
}

/// The structures of an SMBIOS table, each with its strings.
pub fn split(table: &[u8]) -> Vec<Vec<u8>> {
    let mut structures = Vec::new();
    let mut at = 0;
    while at + 4 <= table.len() {
        let length = usize::from(table[at + 1]);
        let end = table[at + length..]
            .windows(2)
            .position(|pair| pair == [0, 0])
            .expect("strings end");
        structures.push(table[at..at + length + end + 2].to_vec());
        at += length + end + 2;
    }
    structures
}
