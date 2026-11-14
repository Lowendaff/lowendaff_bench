//! The device nodes that smartctl may create. Regular files stand
//! in for the nodes.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use bench_host::Root;
use plugin_disk::health::nodes::Nodes;

const NODE: &str = "dev/megaraid_sas_ioctl_node";
const OLD_NODE: &str = "dev/megadev0";

fn root_with_dev() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::create_dir(dir.path().join("dev")).expect("dev is writable");
    dir
}

fn create(dir: &Path, node: &str) {
    fs::write(dir.join(node), "").expect("the stand-in node is writable");
}

#[test]
fn a_node_created_during_the_item_is_removed() {
    let dir = root_with_dev();
    let mut nodes = Nodes::note(&Root::new(dir.path()));
    create(dir.path(), NODE);
    assert!(nodes.remove_created().is_empty(), "removing succeeded");
    assert!(!dir.path().join(NODE).exists());
}

#[test]
fn a_node_that_existed_before_is_kept() {
    let dir = root_with_dev();
    create(dir.path(), NODE);
    let mut nodes = Nodes::note(&Root::new(dir.path()));
    create(dir.path(), OLD_NODE);
    assert!(nodes.remove_created().is_empty());
    assert!(
        dir.path().join(NODE).exists(),
        "it was there before the item"
    );
    assert!(
        !dir.path().join(OLD_NODE).exists(),
        "the old driver's node is handled too"
    );
}

#[test]
fn dropping_removes_a_created_node_on_any_path_out_of_the_item() {
    let dir = root_with_dev();
    let nodes = Nodes::note(&Root::new(dir.path()));
    create(dir.path(), NODE);
    drop(nodes);
    assert!(!dir.path().join(NODE).exists());
}

#[test]
fn a_node_that_cannot_be_removed_is_reported_once() {
    let dir = root_with_dev();
    let mut nodes = Nodes::note(&Root::new(dir.path()));
    // A non-empty directory where the node should be cannot be removed as a file.
    fs::create_dir_all(dir.path().join(NODE).join("inside")).expect("dev is writable");
    let failures = nodes.remove_created();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, dir.path().join(NODE));
    assert!(
        nodes.remove_created().is_empty(),
        "each node is handled once"
    );
}

#[test]
fn a_node_whose_absence_was_not_certain_is_never_removed() {
    let dir = root_with_dev();
    let dev = dir.path().join("dev");
    fs::set_permissions(&dev, fs::Permissions::from_mode(0o000)).expect("dev is ours");
    if fs::read_dir(&dev).is_ok() {
        // Running as root, where permissions do not stop anything: nothing to test.
        fs::set_permissions(&dev, fs::Permissions::from_mode(0o755)).expect("dev is ours");
        return;
    }
    // Permission denied, not "not found": the node might exist.
    let mut nodes = Nodes::note(&Root::new(dir.path()));
    fs::set_permissions(&dev, fs::Permissions::from_mode(0o755)).expect("dev is ours");
    create(dir.path(), NODE);
    assert!(nodes.remove_created().is_empty());
    assert!(dir.path().join(NODE).exists());
}
