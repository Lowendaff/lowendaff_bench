//! Compile-time plugin registry (design D-02). Test items are added here one by one, in
//! default run order.

use bench_api::Registration;

pub const REGISTRY: &[Registration] = &[
    // 系统基础信息: one variant runs, chosen by the detected platform (design D-16).
    bench_api::register::<plugin_system::Dedicated>(),
    bench_api::register::<plugin_system::Vm>(),
    bench_api::register::<plugin_system::Container>(),
    // 硬盘健康: SMART through smartctl, on dedicated servers only. Before 硬盘性能, so that
    // the disks are idle and fio's writes are not counted (design disk-health, DH-08).
    bench_api::register::<plugin_disk_health::Smartctl>(),
    // CPU 性能: sysbench, before fio's writes can disturb it (design cpu, CP-08).
    bench_api::register::<plugin_cpu::Sysbench>(),
    // 硬盘性能: fio with YABS's method, on every platform.
    bench_api::register::<plugin_disk::Fio>(),
];

#[cfg(test)]
mod tests {

    use super::REGISTRY;

    #[test]
    fn registry_passes_the_plugin_contract() {
        assert_eq!(bench_api::validate_registry(REGISTRY), Vec::<String>::new());
    }

    #[test]
    fn the_items_run_in_the_owners_order() {
        // SMART is read while the disks are idle and before fio's writes (design disk-health,
        // DH-08); sysbench runs before fio too (design cpu, CP-08).
        let ids: Vec<&str> = REGISTRY
            .iter()
            .map(|registration| registration.descriptor.id)
            .collect();
        assert_eq!(
            ids,
            [
                "system.dedicated",
                "system.vm",
                "system.container",
                "disk.health",
                "cpu.sysbench",
                "disk.fio",
            ]
        );
    }
}
