//! The external tools of lowendaff-bench, each pinned to one build: the URL, SHA-256 and size of
//! its download for each architecture. This repository builds and publishes its open-source tools,
//! while proprietary or prebuilt vendor suites (Geekbench, PassMark, y-cruncher, Prime95) are
//! downloaded from their upstream distributions and verified by size and SHA-256 before unpacking.
//! Test items take the tools they need from here, so that items sharing a tool share its pin and
//! none depends on another, and run them through the shared code of [`job`] and [`retried`].

#![warn(missing_docs)]

pub mod job;
pub mod retried;

use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;

/// 7-Zip 26.06 (`7zz`) from the release `7zip-26.06-1`, built static without the unRAR codec.
pub const SEVENZIP: ToolSpec = ToolSpec {
    name: "7zip",
    version: "26.06",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/7zip-26.06-1/",
                "7zip-x86_64"
            ),
            sha256: "2ca1d7a93960113b84405ed211fe85f00b797b37927b76bcce0c58ca11646db5",
            size: 2_644_368,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/7zip-26.06-1/",
                "7zip-aarch64"
            ),
            sha256: "11b600d2c362a1e6ffbd4142f29857a5f6b888606f7f0b48fb50168097d5c841",
            size: 2_558_248,
        },
    ],
};

/// fio 3.43 from the release `fio-3.43-2`, which the tools workflow built and published.
pub const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.43",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/fio-3.43-2/",
                "fio-x86_64"
            ),
            sha256: "f5d10f8743eac99b644924302b693deb3f3a1bb3ea3a2572f0efe385e6de9af6",
            size: 6_645_592,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/fio-3.43-2/",
                "fio-aarch64"
            ),
            sha256: "7f592e890267e9d92f9de487263ccccb94addb513a23bb54be18a155ad6ebd77",
            size: 6_813_040,
        },
    ],
};

/// Geekbench 7.1.0 for Linux as Primate Labs publishes it: the release for x86_64 and the
/// preview for aarch64.
pub const GEEKBENCH: ToolSpec = ToolSpec {
    name: "geekbench",
    version: "7.1.0",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "geekbench7",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://cdn.geekbench.com/Geekbench-7.1.0-Linux.tar.gz",
            sha256: "411b72a9c851b69e6f91d390649d419e2e625f475e3983880169651ca27a8672",
            size: 484_515_924,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://cdn.geekbench.com/Geekbench-7.1.0-LinuxARMPreview.tar.gz",
            sha256: "bb6abc61a68a9b33fecf1f3c0b800050dd76e93666a13eaabfa52cbd26ad3319",
            size: 419_635_785,
        },
    ],
};

/// High-Performance Linpack (HPL) 2.3 linked statically against OpenBLAS 0.3.34 (`DYNAMIC_ARCH=1`
/// + OpenMP) and single-node MPI stubs, from the release `hpl-2.3-1`.
pub const HPL: ToolSpec = ToolSpec {
    name: "hpl",
    version: "2.3",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/hpl-2.3-1/",
                "hpl-x86_64"
            ),
            sha256: "f3ea9b272c4469a6f919f6d8a9929afb1126e57f7dd9daa2e15de7946cbcfd49",
            size: 21_250_248,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/hpl-2.3-1/",
                "hpl-aarch64"
            ),
            sha256: "4ea8933757dab2008eab71227f9e07070560ee690419fabb6f09206bf4153cb5",
            size: 12_341_824,
        },
    ],
};

/// iperf3 3.22 from the release `iperf3-3.22-1`, which the tools workflow built and published:
/// static, without OpenSSL and so without iperf3's authentication code, and without SCTP.
pub const IPERF3: ToolSpec = ToolSpec {
    name: "iperf3",
    version: "3.22",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/iperf3-3.22-1/",
                "iperf3-x86_64"
            ),
            sha256: "d3ae394c3b8d2a16c18b86d223339887062881cd89fcd27f3650144c804c9355",
            size: 253_848,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/iperf3-3.22-1/",
                "iperf3-aarch64"
            ),
            sha256: "2d82185e2706cce73a01f90f43bbf4e231032af06bba1b5fc8255cb5039865b4",
            size: 265_992,
        },
    ],
};

/// multichase and multiload from the release `multichase-2026.10.01-1`.
pub const MULTICHASE: ToolSpec = ToolSpec {
    name: "multichase",
    version: "2026.10.01",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "multichase",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "multichase-2026.10.01-1/multichase-x86_64.tar.gz"
            ),
            sha256: "8a2cb5f805c2faa194852e2504f7e563592ced3d3a5da74c04b14428d4bc00d0",
            size: 976_234,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "multichase-2026.10.01-1/multichase-aarch64.tar.gz"
            ),
            sha256: "5d4188157c5579c88994a9de8df5ddcd96cfef639da5510066a61f2892aee915",
            size: 885_268,
        },
    ],
};

/// NextTrace 1.7.6 from the release `nexttrace-1.7.6-1`, which the tools workflow built and
/// published: static, its tiny flavor, with Go's fallback roots for a host without root
/// certificates of its own. It traces routes with raw ICMP packets, so the program gives its copy
/// CAP_NET_RAW, and the item runs it as an unprivileged user.
pub const NEXTTRACE: ToolSpec = ToolSpec {
    name: "nexttrace",
    version: "1.7.6",
    all_arches: true,
    raw_sockets: true,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/nexttrace-1.7.6-1/",
                "nexttrace-x86_64"
            ),
            sha256: "834364c4bf51a8537c9313a36dd9dfacdb00bc3490c5ba83f2e6d8fa01477a4d",
            size: 15_036_540,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/nexttrace-1.7.6-1/",
                "nexttrace-aarch64"
            ),
            sha256: "78b7054a8255b29dfa46201942600e4a6383ad2f6663e4fa8808269c3c75a100",
            size: 13_959_292,
        },
    ],
};

/// PassMark PerformanceTest Linux 11.0 Build 1004, downloaded directly from PassMark Software.
pub const PASSMARK: ToolSpec = ToolSpec {
    name: "passmark",
    version: "11.0.1004",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Zip {
        x86_64: "PerformanceTest_Linux_x86-64",
        aarch64: "PerformanceTest_Linux_ARM64",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: "https://www.passmark.com/downloads/PerformanceTest_Linux_x86-64.zip",
            sha256: "dab991f957373a5cfaa1e1b212a882bbb4d0fb85abaa22fc7694e8bd9dc070ad",
            size: 5_646_190,
        },
        ToolAsset {
            arch: "aarch64",
            url: "https://www.passmark.com/downloads/PerformanceTest_Linux_ARM64.zip",
            sha256: "968d247170df1a060e798ce0e9c033c83c8e018fafbd2930d4db666c7d435021",
            size: 2_588_994,
        },
    ],
};

/// Open-source runtime libraries (`libncurses.so.5`, `xterm` terminfo, and `libcurl.so.4`) built
/// against glibc 2.28 in `passmark-libs-1.0-1` so PassMark runs on distros without `libncurses5`.
pub const PASSMARK_LIBS: ToolSpec = ToolSpec {
    name: "passmark-libs",
    version: "1.0",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "libncurses.so.5",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "passmark-libs-1.0-1/passmark-libs-x86_64.tar.gz"
            ),
            sha256: "a965f18bb5038f5a9cb3cfba21f43777e5ab1985afd96953de1de8420fdb06a6",
            size: 407_002,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "passmark-libs-1.0-1/passmark-libs-aarch64.tar.gz"
            ),
            sha256: "4a2b6fe85c0113c2c65144a775da83167f13009c182fe6bb1a9fa273b4ed1a9f",
            size: 399_976,
        },
    ],
};

/// Prime95 (`mprime`) 30.19 build 20 for Linux x86_64, downloaded from GIMPS (`mersenne.org`).
pub const PRIME95: ToolSpec = ToolSpec {
    name: "prime95",
    version: "30.19b20",
    all_arches: false,
    raw_sockets: false,
    form: ToolForm::TarGzFlat {
        executable: "mprime",
    },
    assets: &[ToolAsset {
        arch: "x86_64",
        url: "https://www.mersenne.org/download/software/v30/30.19/p95v3019b20.linux64.tar.gz",
        sha256: "4ce2377e03deb4cf189523136e26401ba08f67857a128e420dd030d00cdca601",
        size: 5_912_959,
    }],
};

/// smartctl 7.5 from the release `smartctl-7.5-1`, which the smartctl workflow built and
/// published.
pub const SMARTCTL: ToolSpec = ToolSpec {
    name: "smartctl",
    version: "7.5",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/smartctl-7.5-1/",
                "smartctl-x86_64"
            ),
            sha256: "dbf17079b2b8469ede7002a7b15ac4b65f833cae658f229a3ba8ed158e4d6345",
            size: 7_447_664,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/smartctl-7.5-1/",
                "smartctl-aarch64"
            ),
            sha256: "ea4c64b5c38678fc40847598d83537395b7749ee7f274e8c228dd09172ae962f",
            size: 7_359_608,
        },
    ],
};

/// STREAM 5.10 from the release `stream-5.10-1`: six programs of one size each, the smallest
/// named; the memory item runs the others beside it.
pub const STREAM: ToolSpec = ToolSpec {
    name: "stream",
    version: "5.10",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "stream-2m",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "stream-5.10-1/stream-x86_64.tar.gz"
            ),
            sha256: "cd0bba47b10c6ef5e02cb7a5218ad65183d83f23061b0cc71cf0128b0a1e585f",
            size: 1_609_410,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "stream-5.10-1/stream-aarch64.tar.gz"
            ),
            sha256: "3f422aef2895bd424071c0d2d398c379c2cb391ac671b018ec3b033087c43fa7",
            size: 1_686_248,
        },
    ],
};

/// sysbench 1.0.20 from the release `sysbench-1.0.20-1`, which the tools workflow built and
/// published.
pub const SYSBENCH: ToolSpec = ToolSpec {
    name: "sysbench",
    version: "1.0.20",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::Executable,
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "sysbench-1.0.20-1/sysbench-x86_64"
            ),
            sha256: "a090a82b15b74efaffa2d80c8e1a74c28127ce38c4f8512e7fddcc13d3b5fd78",
            size: 1_779_416,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "sysbench-1.0.20-1/sysbench-aarch64"
            ),
            sha256: "0d132f10716254e24ada1b9c4e7b0d37c5cadfc42e32bfaf9a7483ee8789569f",
            size: 1_879_248,
        },
    ],
};

/// Byte-UnixBench 6.0.1 system index programs (`dhry2reg`, `whetstone-double`, `execl`, `fstime`,
/// `pipe`, `context1`, `spawn`, `syscall`, `looper`, `multi.sh`, `tst.sh`, `sort.src`,
/// `index.base`) built static in `unixbench-6.0.1-1`.
pub const UNIXBENCH: ToolSpec = ToolSpec {
    name: "unixbench",
    version: "6.0.1",
    all_arches: true,
    raw_sockets: false,
    form: ToolForm::TarGz {
        executable: "dhry2reg",
    },
    assets: &[
        ToolAsset {
            arch: "x86_64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "unixbench-6.0.1-1/unixbench-x86_64.tar.gz"
            ),
            sha256: "bd7a8951ea3939c77d8eeaddbe88f43151e3fd4b0b5a86bbcaf38eb7295003f8",
            size: 137_554,
        },
        ToolAsset {
            arch: "aarch64",
            url: concat!(
                "https://github.com/Lowendaff/lowendaff_bench/releases/download/",
                "unixbench-6.0.1-1/unixbench-aarch64.tar.gz"
            ),
            sha256: "8408bdde367b51dc0486c9f7a771fcf50a1fa9e04eeb9aa59402977128c84aba",
            size: 238_521,
        },
    ],
};

/// y-cruncher 0.8.7 Build 9547 static Linux x86_64 release.
pub const YCRUNCHER: ToolSpec = ToolSpec {
    name: "ycruncher",
    version: "0.8.7.9547",
    all_arches: false,
    raw_sockets: false,
    form: ToolForm::TarXz {
        executable: "y-cruncher",
    },
    assets: &[ToolAsset {
        arch: "x86_64",
        url: concat!(
            "https://github.com/Mysticial/y-cruncher/releases/download/v0.8.7.9547/",
            "y-cruncher.v0.8.7.9547-static.tar.xz"
        ),
        sha256: "e22ff344f42763520d2331f19ed6177f3a7c78ac5bd462e9c060c38267181490",
        size: 35_954_568,
    }],
};
