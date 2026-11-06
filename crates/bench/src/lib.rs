//! lowendaff-bench orchestrator: plans plugins, provisions their tools, runs them in
//! order and prints the results. Design: `docs/design/plugin-system.md`.

pub mod app;
pub mod fetch;
pub mod orchestrator;
pub mod probe;
pub mod provision;
pub mod registry;
pub mod render;
