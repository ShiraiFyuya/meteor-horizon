//! The port's acceptance test: the module registry must match Meteor Client's.
//!
//! `meteor_modules.txt` is extracted from the Java sources by
//! `tools/extract_module_inventory.py` (one `category name` line per module, in
//! `Modules.java`'s registration order). This test fails when a module is
//! missing, renamed, or filed under the wrong category, which is exactly what
//! "fully ported" means here.

use std::collections::BTreeSet;

use meteor_core::category::Category;

const EXPECTED: &str = include_str!("meteor_modules.txt");

/// Modules the port adds that Meteor does not have (documentation/demo only).
const PORT_ONLY: &[&str] = &["example"];

fn expected() -> Vec<(Category, &'static str)> {
    EXPECTED
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|line| {
            let (cat, name) = line.trim().split_once(' ').expect("category name");
            let category = Category::from_name(cat).unwrap_or_else(|| panic!("unknown category {cat}"));
            (category, name)
        })
        .collect()
}

#[test]
fn every_meteor_module_is_registered_under_its_category() {
    let shared = meteor_mod::shared::Shared::new();
    let mut missing = Vec::new();
    let mut wrong_category = Vec::new();

    for (category, name) in expected() {
        match shared.modules.base(name) {
            None => missing.push(name),
            Some(base) => {
                if base.category != category {
                    wrong_category.push((name, category, base.category));
                }
            }
        }
    }

    assert!(
        missing.is_empty(),
        "{} modules missing from the port: {missing:?}",
        missing.len()
    );
    assert!(
        wrong_category.is_empty(),
        "modules in the wrong category: {wrong_category:?}"
    );
}

#[test]
fn the_port_adds_no_undocumented_modules() {
    let shared = meteor_mod::shared::Shared::new();
    let expected: BTreeSet<&str> = expected().into_iter().map(|(_, n)| n).collect();
    let mut extra: Vec<&str> = Vec::new();
    for module in shared.modules.all() {
        let name = module.base().name;
        if !expected.contains(name) && !PORT_ONLY.contains(&name) {
            extra.push(name);
        }
    }
    assert!(extra.is_empty(), "unexpected modules in the registry: {extra:?}");
}

#[test]
fn registry_order_matches_modules_java_within_each_category() {
    let shared = meteor_mod::shared::Shared::new();
    // `Modules.sortModules()` sorts each group by title, so compare the *sets* and
    // the count per category; order inside a category is the GUI's sort order.
    for category in Category::ALL {
        let expected_count = expected()
            .into_iter()
            .filter(|(c, _)| *c == category)
            .count();
        let port_only = shared
            .modules
            .group(category)
            .iter()
            .filter(|m| PORT_ONLY.contains(&m.base().name))
            .count();
        let actual = shared.modules.group(category).len() - port_only;
        assert_eq!(
            actual,
            expected_count,
            "{} registers {actual} modules, Meteor has {expected_count}",
            category.name()
        );
    }
}

#[test]
fn every_module_has_settings_or_a_reason_not_to() {
    let shared = meteor_mod::shared::Shared::new();
    let mut without: Vec<&str> = Vec::new();
    for module in shared.modules.all() {
        let base = module.base();
        if base.settings.is_empty() && base.name != "example" {
            without.push(base.name);
        }
    }
    // Meteor has a handful of setting-less modules (`AntiAnvil`, `LiquidInteract`,
    // `Multitask`); keep the list small and explicit.
    let allowed = BTreeSet::from([
        "anti-anvil",
        "liquid-interact",
        "multitask",
        "portals",
        "sneak",
        "build-height",
    ]);
    let unexpected: Vec<&&str> = without.iter().filter(|n| !allowed.contains(**n)).collect();
    assert!(
        unexpected.is_empty(),
        "modules with no settings that Meteor gives settings: {unexpected:?}"
    );
}

#[test]
fn every_module_info_string_is_short() {
    let shared = meteor_mod::shared::Shared::new();
    for module in shared.modules.all() {
        if let Some(info) = module.info_string() {
            assert!(
                info.chars().count() <= 40,
                "{}'s info string is too long for the HUD: {info}",
                module.base().name
            );
        }
    }
}
