//! fio's jobs against YABS's commands, and the parser against fio 3.43's real output
//! (`tests/fixtures/README.md`).

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use bench_tool::job;
use plugin_disk::fio::Fault;
use plugin_disk::fio::measure;
use plugin_disk::fio::parse;
use plugin_disk::fio::result::Outcome;
use plugin_disk::fio::result::Rates;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fio")
        .join(name);
    fs::read_to_string(&path).expect("fixture is readable")
}

fn finished(text: &str) -> job::Output {
    job::Output {
        code: Some(0),
        text: text.to_owned(),
    }
}

fn strings(args: &[OsString]) -> Vec<String> {
    let mut strings = Vec::new();
    for arg in args {
        strings.push(arg.to_str().expect("the test path is UTF-8").to_owned());
    }
    strings
}

#[test]
fn the_setup_run_is_yabss() {
    // yabs.sh v2026-10-31, disk_test, with FIO_SIZE=2G and DISK_PATH=/run/disk.
    let args = measure::setup_args(Path::new("/run/disk/test.fio"), "2G");
    assert_eq!(
        strings(&args),
        [
            "--name=setup",
            "--ioengine=libaio",
            "--rw=read",
            "--bs=64k",
            "--iodepth=64",
            "--numjobs=2",
            "--size=2G",
            "--runtime=1",
            "--gtod_reduce=1",
            "--filename=/run/disk/test.fio",
            "--direct=1",
            "--minimal",
        ]
    );
}

#[test]
fn every_test_run_is_yabss() {
    let mut block_sizes = Vec::new();
    for pair in measure::BLOCK_SIZES {
        block_sizes.extend(pair);
    }
    assert_eq!(
        block_sizes,
        ["4k", "64k", "512k", "1m"],
        "YABS's BLOCK_SIZES"
    );
    for block_size in block_sizes {
        let args = measure::test_args(Path::new("/run/disk/test.fio"), "512M", block_size);
        assert_eq!(
            strings(&args),
            [
                format!("--name=rand_rw_{block_size}").as_str(),
                "--ioengine=libaio",
                "--rw=randrw",
                "--rwmixread=50",
                format!("--bs={block_size}").as_str(),
                "--iodepth=64",
                "--numjobs=2",
                "--size=512M",
                "--runtime=30",
                "--gtod_reduce=1",
                "--direct=1",
                "--filename=/run/disk/test.fio",
                "--group_reporting",
                "--minimal",
            ]
        );
    }
}

#[test]
fn colons_in_the_test_path_are_escaped() {
    let args = strings(&measure::setup_args(
        Path::new("/srv/a:b/c:d/test.fio"),
        "2G",
    ));
    assert!(
        args.contains(&r"--filename=/srv/a\:b/c\:d/test.fio".to_owned()),
        "fio splits --filename at unescaped colons: {args:?}"
    );
}

#[test]
fn rates_come_from_terse_fields_7_8_48_and_49() {
    for (name, job, rates) in [
        (
            "rand_rw_4k-aarch64.txt",
            "rand_rw_4k",
            (597_651, 149_412, 604_844, 151_211),
        ),
        (
            "rand_rw_64k-aarch64.txt",
            "rand_rw_64k",
            (4_834_461, 75_538, 5_248_000, 82_000),
        ),
        (
            "rand_rw_512k-aarch64.txt",
            "rand_rw_512k",
            (10_325_333, 20_166, 11_520_000, 22_500),
        ),
        (
            "rand_rw_1m-aarch64.txt",
            "rand_rw_1m",
            (8_338_285, 8_142, 10_386_285, 10_142),
        ),
        (
            "rand_rw_4k-x86_64.txt",
            "rand_rw_4k",
            (597_651, 149_412, 604_844, 151_211),
        ),
        (
            "rand_rw_64k-x86_64.txt",
            "rand_rw_64k",
            (4_189_866, 65_466, 4_548_266, 71_066),
        ),
        (
            "rand_rw_512k-x86_64.txt",
            "rand_rw_512k",
            (8_850_285, 17_285, 9_874_285, 19_285),
        ),
        (
            "rand_rw_1m-x86_64.txt",
            "rand_rw_1m",
            (5_306_181, 5_181, 6_609_454, 6_454),
        ),
        (
            "rand_rw_4k-note.txt",
            "rand_rw_4k",
            (112_394, 28_098, 118_366, 29_591),
        ),
    ] {
        let (read_kib, read_iops, write_kib, write_iops) = rates;
        let expected = Rates {
            read_kib,
            read_iops,
            write_kib,
            write_iops,
        };
        assert_eq!(parse::rates(&fixture(name), job), Ok(expected), "{name}");
        assert_eq!(
            parse::outcome(&finished(&fixture(name)), job),
            Outcome::Done(expected),
            "{name}"
        );
        assert_eq!(
            parse::version(&fixture(name), job).as_deref(),
            Some("3.43"),
            "{name}"
        );
    }
}

#[test]
fn a_result_for_another_job_does_not_count() {
    let output = fixture("rand_rw_4k-aarch64.txt");
    assert_eq!(parse::rates(&output, "rand_rw_64k"), Err(Fault::NoResult));
    assert_eq!(parse::rates("", "rand_rw_4k"), Err(Fault::NoResult));
    assert_eq!(parse::version(&output, "rand_rw_64k"), None);
}

#[test]
fn an_error_in_the_result_line_is_reported() {
    // The first job of the setup run hit ENOSPC (error 28) while laying out the file.
    let output = fixture("setup-full.txt");
    assert_eq!(parse::rates(&output, "setup"), Err(Fault::Error(28)));
    let error =
        "fio: pid=0, err=28/file:filesetup.c:241, func=write, error=No space left on device";
    let failed = job::Output {
        code: Some(1),
        text: output,
    };
    assert_eq!(
        parse::outcome(&failed, "setup"),
        Outcome::Failed(Fault::Tool(job::Fault::Exit {
            code: 1,
            last: Some(error.to_owned()),
        }))
    );
    let signalled = job::Output {
        code: None,
        ..failed
    };
    assert_eq!(
        parse::fault(&signalled),
        Some(Fault::Tool(job::Fault::Signal {
            last: Some(error.to_owned()),
        }))
    );
}

#[test]
fn a_field_that_is_not_a_number_is_reported() {
    let output = fixture("rand_rw_1m-aarch64.txt").replacen(";8338285;", ";8338285.5;", 1);
    assert_eq!(parse::rates(&output, "rand_rw_1m"), Err(Fault::Field(7)));
    let short = "3;fio-3.43;rand_rw_1m;0;0;58368;8338285;8142";
    assert_eq!(parse::rates(short, "rand_rw_1m"), Err(Fault::Field(48)));
}

#[test]
fn a_filesystem_without_direct_io_is_recognized() {
    assert!(parse::lacks_direct_io(&fixture("setup-ramfs.txt")));
    assert!(parse::lacks_direct_io(
        "fio: looks like your file system does not support direct=1/buffered=0"
    ));
    assert!(parse::lacks_direct_io(
        "fio: destination does not support O_DIRECT"
    ));
    assert!(!parse::lacks_direct_io(&fixture("setup-full.txt")));
    assert!(!parse::lacks_direct_io(&fixture("rand_rw_4k-aarch64.txt")));
}

#[test]
fn the_error_line_skips_results_in_either_order() {
    let ramfs = fixture("setup-ramfs.txt");
    let expected = "fio: pid=21, err=22/file:filesetup.c:800, func=open(/r/test.fio), \
                    error=Invalid argument";
    assert_eq!(parse::error_line(&ramfs), Some(expected));
    // run_fio puts standard output, with the results, first.
    let (results, messages): (Vec<&str>, Vec<&str>) =
        ramfs.lines().partition(|line| line.starts_with("3;"));
    let reordered = format!("{}\n{}\n", results.join("\n"), messages.join("\n"));
    assert_eq!(parse::error_line(&reordered), Some(expected));
    assert_eq!(parse::error_line(" \n\n"), None);
}

#[test]
fn a_version_is_read_only_from_a_plain_number() {
    let line = |version: &str| format!("3;{version};setup;0;0");
    assert_eq!(
        parse::version(&line("fio-3.43"), "setup").as_deref(),
        Some("3.43")
    );
    assert_eq!(parse::version(&line("fio-3.43-dirty"), "setup"), None);
    assert_eq!(parse::version(&line("3.43"), "setup"), None);
    assert_eq!(parse::version(&line("fio-"), "setup"), None);
}
