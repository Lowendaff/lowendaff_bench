//! Choosing what to trace: by default both cache servers and the list's defaults; by group, every
//! target of the group; by id, those targets and cache servers alone, with the ids that nothing
//! has.

use plugin_network::trace::Choice;
use plugin_network::trace::targets;
use plugin_network::trace::targets::Chosen;
use plugin_network::trace::targets::Group;
use plugin_network::trace::targets::Target;

fn target(id: &str, group: Group, default: bool) -> Target {
    Target {
        id: id.to_owned(),
        group,
        name: id.to_uppercase(),
        asn: 64496,
        ipv4: format!("{id}.example"),
        ipv6: None,
        default,
    }
}

/// A list of two targets in China, one of them no default, and one elsewhere.
fn list() -> Vec<Target> {
    vec![
        target("pek", Group::China, true),
        target("sha", Group::China, false),
        target("fra", Group::International, true),
    ]
}

/// The ids of what `chosen` traces, the cache servers first.
fn ids(chosen: &Chosen) -> Vec<String> {
    let mut ids = Vec::new();
    if chosen.youtube {
        ids.push("youtube".to_owned());
    }
    if chosen.netflix {
        ids.push("netflix".to_owned());
    }
    ids.extend(chosen.targets.iter().map(|target| target.id.clone()));
    ids
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn by_default_both_cache_servers_and_the_lists_defaults_are_traced() {
    let chosen = targets::choose(&list(), &[], &[]);
    assert_eq!(ids(&chosen), ["youtube", "netflix", "pek", "fra"]);
    assert!(chosen.unknown.is_empty());
}

#[test]
fn a_group_chooses_every_target_of_it_and_all_chooses_everything() {
    let by = |groups: &[Choice]| ids(&targets::choose(&list(), groups, &[]));
    assert_eq!(by(&[Choice::China]), ["pek", "sha"]);
    assert_eq!(by(&[Choice::International]), ["fra"]);
    assert_eq!(by(&[Choice::Services]), ["youtube", "netflix"]);
    assert_eq!(
        by(&[Choice::International, Choice::Services]),
        ["youtube", "netflix", "fra"]
    );
    assert_eq!(
        by(&[Choice::All]),
        ["youtube", "netflix", "pek", "sha", "fra"]
    );
    assert_eq!(
        by(&[Choice::China, Choice::International]),
        ["pek", "sha", "fra"],
        "in the list's order"
    );
}

#[test]
fn ids_win_over_groups_and_name_the_cache_servers_too() {
    let chosen = targets::choose(
        &list(),
        &[Choice::All],
        &strings(&["fra", "youtube", "xyz", "sha"]),
    );
    assert_eq!(ids(&chosen), ["youtube", "sha", "fra"]);
    assert_eq!(chosen.unknown, ["xyz"]);
    let netflix = targets::choose(&list(), &[], &strings(&["netflix"]));
    assert_eq!(ids(&netflix), ["netflix"]);
    assert!(netflix.unknown.is_empty());
    let nothing = targets::choose(&list(), &[], &strings(&["xyz", "abc"]));
    assert!(ids(&nothing).is_empty());
    assert_eq!(nothing.unknown, ["xyz", "abc"]);
}

#[test]
fn the_cache_servers_are_chosen_as_their_files_are_asked_for() {
    assert_eq!(targets::services(&[], &[]), (true, true));
    assert_eq!(targets::services(&[Choice::China], &[]), (false, false));
    assert_eq!(targets::services(&[Choice::All], &[]), (true, true));
    assert_eq!(targets::services(&[Choice::Services], &[]), (true, true));
    assert_eq!(
        targets::services(&[Choice::All], &strings(&["netflix"])),
        (false, true)
    );
    assert_eq!(
        targets::services(&[], &strings(&["youtube", "pek"])),
        (true, false)
    );
    assert_eq!(targets::services(&[], &strings(&["pek"])), (false, false));
}
