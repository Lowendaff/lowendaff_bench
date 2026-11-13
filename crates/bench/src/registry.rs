//! Compile-time plugin registry. Test items are added here one by one, in
//! default run order.

use bench_api::Registration;

pub const REGISTRY: &[Registration] = &[
    // 系统基础信息: one variant runs, chosen by the detected platform.
    bench_api::register::<plugin_system::Dedicated>(),
    bench_api::register::<plugin_system::Vm>(),
    bench_api::register::<plugin_system::Container>(),
    // 硬盘健康: SMART through smartctl, on dedicated servers only. Before 硬盘性能, so that
    // the disks are idle and fio's writes are not counted.
    bench_api::register::<plugin_disk_health::Smartctl>(),
    // CPU 性能: sysbench, before fio's writes can disturb it.
    bench_api::register::<plugin_cpu::Sysbench>(),
    // 内存性能: sysbench, STREAM and multichase, after CPU 性能 and before fio's writes.
    bench_api::register::<plugin_memory::Memory>(),
    // 硬盘性能: fio with YABS's method, on every platform.
    bench_api::register::<plugin_disk::Fio>(),
    // Geekbench 7: last, being the longest, with a download, an upload and perhaps swap that
    // must not disturb the disk test.
    bench_api::register::<plugin_geekbench::Geekbench>(),
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
        // SMART is read while the disks are idle and before fio's writes; sysbench runs before
        // fio too.
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
                "memory",
                "disk.fio",
                "cpu.geekbench",
            ]
        );
    }
}
