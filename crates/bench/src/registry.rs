//! Compile-time plugin registry (design D-02). Test items are added here one by one, in
//! default run order.

use bench_api::Registration;

pub const REGISTRY: &[Registration] = &[];

#[cfg(test)]
mod tests {
    use bench_api::validate_registry;

    use super::REGISTRY;

    #[test]
    fn registry_passes_the_plugin_contract() {
        assert_eq!(validate_registry(REGISTRY), Vec::<String>::new());
    }
}
