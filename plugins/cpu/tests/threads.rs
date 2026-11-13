//! The CPUs that the CPU items use: the host profile's count, else the scheduler's.

use std::num::NonZeroUsize;

use bench_host::cpu::Cpu;
use bench_host::profile::CpuFacts;
use bench_host::profile::HostProfile;
use bench_host::profile::Read;

fn host(cpu: Read<CpuFacts>) -> HostProfile {
    HostProfile {
        cpu,
        ..HostProfile::default()
    }
}

fn counted(usable: Option<usize>) -> Read<CpuFacts> {
    Read::Done(CpuFacts {
        cpu: Cpu::default(),
        usable,
        quota: None,
    })
}

#[test]
fn the_profiles_count_comes_first_and_the_schedulers_without_it() {
    // A count no machine running the tests has, so that it can only come from the profile.
    assert_eq!(plugin_cpu::usable_threads(&host(counted(Some(997)))), 997);
    let scheduler = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    for cpu in [counted(None), Read::TimedOut, Read::Failed] {
        assert_eq!(plugin_cpu::usable_threads(&host(cpu)), scheduler);
    }
}
