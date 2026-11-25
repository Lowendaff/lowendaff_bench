//! Language selection, CLI code parsing, scoped overrides, and four-language catalog checks.

use std::collections::BTreeSet;
use std::panic;

use bench::locale;
use bench::locale::HostHeader;
use bench::locale::HostLabel;
use bench::locale::IpTable;
use bench::locale::Language;
use bench::registry;

#[test]
fn default_language_is_english_and_parse_language_accepts_codes_and_aliases() {
    assert_eq!(locale::current_language(), Language::En);
    assert_eq!(locale::parse_language("en"), Some(Language::En));
    assert_eq!(locale::parse_language("EN"), Some(Language::En));
    assert_eq!(locale::parse_language("zh-cn"), Some(Language::ZhCn));
    assert_eq!(locale::parse_language("ZH-HANS"), Some(Language::ZhCn));
    assert_eq!(locale::parse_language("zh-tw"), Some(Language::ZhTw));
    assert_eq!(locale::parse_language("zh-hant"), Some(Language::ZhTw));
    assert_eq!(locale::parse_language("zh-hk"), Some(Language::ZhTw));
    assert_eq!(locale::parse_language("ja"), Some(Language::Ja));
    assert_eq!(locale::parse_language("fr"), None);
    assert_eq!(locale::parse_language(""), None);
}

#[test]
fn with_language_scopes_set_language_to_current_thread_and_restores_on_unwind() {
    locale::with_language(Language::En, || {
        assert_eq!(locale::current_language(), Language::En);
        locale::set_language(Language::ZhCn);
        assert_eq!(locale::current_language(), Language::ZhCn);
        locale::with_language(Language::En, || {
            assert_eq!(locale::current_language(), Language::En);
        });
        assert_eq!(locale::current_language(), Language::ZhCn);

        let unwound = panic::catch_unwind(|| {
            locale::with_language(Language::Ja, || {
                assert_eq!(locale::current_language(), Language::Ja);
                panic!("simulated unwind");
            });
        });
        assert!(unwound.is_err());
        assert_eq!(locale::current_language(), Language::ZhCn);
    });
    assert_eq!(locale::current_language(), Language::En);
}

#[test]
fn english_counts_in_the_singular_for_one() {
    let words = locale::catalog_for(Language::En);
    assert_eq!(words.item_count(1), "1 test item");
    assert_eq!(words.item_count(8), "8 test items");
}

#[test]
fn every_catalog_names_all_items_groups_and_distinct_localized_terminology() {
    let entries = registry::all();
    let groups = ["cpu", "memory", "disk", "network", "ip"];

    for lang in Language::ALL {
        let words = locale::catalog_for(lang);
        assert!(words.label_width() >= 18, "{lang:?} label_width");
        assert!(!words.about().trim().is_empty(), "{lang:?} about");
        assert!(!words.help_lang().trim().is_empty(), "{lang:?} help_lang");
        assert!(
            !words.system_title().trim().is_empty(),
            "{lang:?} system_title"
        );
        assert!(
            !words.summary_title().trim().is_empty(),
            "{lang:?} summary_title"
        );
        assert!(
            !words.tui_field_language().trim().is_empty(),
            "{lang:?} tui_field_language"
        );
        assert!(
            !words.tui_lang_picker_title().trim().is_empty(),
            "{lang:?} tui_lang_picker_title"
        );
        assert!(
            words.tui_keys_config_list().contains("L:"),
            "{lang:?} config list keybar must advertise L shortcut"
        );
        assert!(
            words.tui_keys_results().contains("L:"),
            "{lang:?} results keybar must advertise L shortcut"
        );

        let mut seen_groups = BTreeSet::new();
        for group in groups {
            let title = words
                .group_title(group)
                .unwrap_or_else(|| panic!("{lang:?} missing group_title for {group}"));
            assert!(!title.trim().is_empty(), "{lang:?} empty group {group}");
            assert!(
                seen_groups.insert(title),
                "{lang:?} duplicate group title `{title}`"
            );
        }

        let mut seen_items = BTreeSet::new();
        for entry in &entries {
            let name = words
                .item_name(entry.id)
                .unwrap_or_else(|| panic!("{lang:?} missing item_name for {}", entry.id));
            assert!(!name.trim().is_empty(), "{lang:?} empty item {}", entry.id);
            assert!(
                !name.chars().any(char::is_control),
                "{lang:?} control char in {}",
                entry.id
            );
            assert!(
                seen_items.insert(name),
                "{lang:?} duplicate item name `{name}` on {}",
                entry.id
            );
        }

        for header in [
            words.host_label(HostLabel::Memory),
            words.host_label(HostLabel::Filesystems),
            words.host_header(HostHeader::MountPoint),
            words.ip_table(IpTable::Summary),
        ] {
            assert!(!header.trim().is_empty(), "{lang:?} empty label");
        }
    }

    let en = locale::catalog_for(Language::En);
    let zh_cn = locale::catalog_for(Language::ZhCn);
    let zh_tw = locale::catalog_for(Language::ZhTw);
    let ja = locale::catalog_for(Language::Ja);

    assert_ne!(en.system_title(), zh_cn.system_title());
    assert_ne!(en.system_title(), ja.system_title());
    assert_ne!(zh_cn.system_title(), ja.system_title());
    // Simplified vs Traditional Chinese use distinct idiomatic terms (memory: Neicun vs Jiyiti).
    assert!(
        zh_cn
            .item_name("memory.sysbench")
            .unwrap()
            .contains("\u{5185}\u{5b58}")
    );
    assert!(
        zh_tw
            .item_name("memory.sysbench")
            .unwrap()
            .contains("\u{8a18}\u{61b6}\u{9ad4}")
    );
    assert!(
        ja.item_name("memory.sysbench")
            .unwrap()
            .contains("\u{30e1}\u{30e2}\u{30ea}")
    );
}
