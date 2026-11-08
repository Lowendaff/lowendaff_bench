//! Compile-time plugin registry (design D-02). Test items are added here one by one, in
//! default run order.

use bench_api::Registration;

pub const REGISTRY: &[Registration] = &[
    // 系统基础信息: one variant runs, chosen by the detected platform (design D-16).
    bench_api::register::<plugin_system::Dedicated>(),
    bench_api::register::<plugin_system::Vm>(),
    bench_api::register::<plugin_system::Container>(),
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
}
