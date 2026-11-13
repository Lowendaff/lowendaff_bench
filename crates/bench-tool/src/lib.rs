//! The external tools of lowendaff-bench, each pinned to one build: the URL, SHA-256 and size of
//! its download for each architecture. This repository builds and publishes every tool except
//! Geekbench, whose own builds are downloaded from Primate Labs. Test items take the tools they
//! need from here, so that items sharing a tool share its pin and none depends on another, and
//! run them through the shared code of [`job`] and [`retried`].

#![warn(missing_docs)]

pub mod job;
pub mod retried;

use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;

/// fio 3.43 from the release `fio-3.43-2`, which the tools workflow built and published.
pub const FIO: ToolSpec = ToolSpec {
    name: "fio",
    version: "3.43",
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

/// multichase and multiload from the release `multichase-2026.10.01-1`.
pub const MULTICHASE: ToolSpec = ToolSpec {
    name: "multichase",
    version: "2026.10.01",
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

/// smartctl 7.5 from the release `smartctl-7.5-1`, which the smartctl workflow built and
/// published.
pub const SMARTCTL: ToolSpec = ToolSpec {
    name: "smartctl",
    version: "7.5",
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
