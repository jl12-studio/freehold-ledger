use std::path::Path;

fn repo_file(name: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(name)).unwrap()
}

/// Markdown renumbers ordered lists from their first number, so a gap such as
/// "1. 3. 5." renders as 1, 2, 3 and misstates which Standard points apply.
#[test]
fn readme_ordered_lists_have_no_gaps() {
    let readme = repo_file("README.md");
    let mut prev: Option<u32> = None;
    for line in readme.lines() {
        let n = line.split_once(". ").and_then(|(n, _)| n.parse::<u32>().ok());
        match (prev, n) {
            (Some(p), Some(n)) => assert_eq!(n, p + 1, "README list jumps from {p} to {n}: {line}"),
            _ => {}
        }
        prev = n;
    }
}

#[test]
fn data_model_lists_every_category_key() {
    let doc = repo_file("DATA-MODEL.md");
    for c in ledger_core::categories() {
        assert!(doc.contains(&format!("| `{}` |", c.key)), "DATA-MODEL.md is missing {}", c.key);
    }
}

const TOOLS: [&str; 29] = [
    "archive_business", "assign_payment_method", "backup_ledger", "correct_transaction",
    "create_business", "create_payment_method", "export_tax_year", "ledger_info", "list_backups",
    "list_businesses",
    "list_categories", "list_payment_methods", "list_transactions", "reclassify_transaction",
    "record_expense", "record_income", "record_transactions", "rename_business",
    "rename_payment_method", "restore_business", "restore_ledger", "retire_payment_method",
    "set_default_payment_method", "summarize", "switch_business", "update_business",
    "verify_ledger", "void_batch", "void_transaction",
];

#[test]
fn agent_guide_lists_every_tool() {
    let guide = repo_file("docs/AGENT-GUIDE.md");
    for t in TOOLS {
        assert!(guide.contains(&format!("| `{t}` |")), "AGENT-GUIDE.md is missing {t}");
    }
}

#[test]
fn guides_have_no_em_dash() {
    for f in ["README.md", "DATA-MODEL.md", "docs/AGENT-GUIDE.md", "docs/HUMAN-GUIDE.md"] {
        assert!(!repo_file(f).contains('\u{2014}'), "{f} contains an em dash");
    }
}

#[test]
fn data_model_names_payment_method_tables() {
    let doc = repo_file("DATA-MODEL.md");
    for t in ["payment_methods", "payment_method_changes", "default_payment_method_changes", "payment_method_assignments"] {
        assert!(doc.contains(&format!("`{t}`")), "DATA-MODEL.md is missing {t}");
    }
}

#[test]
fn data_model_names_business_tables() {
    let doc = repo_file("DATA-MODEL.md");
    for t in [
        "businesses", "business_changes", "current_business_changes", "transaction_businesses",
        "batch_businesses", "payment_method_businesses", "business_default_payment_method_changes",
    ] {
        assert!(doc.contains(&format!("`{t}`")), "DATA-MODEL.md is missing {t}");
    }
}

/// bundle/dmg keeps disk images from earlier builds, so the release script must pick this
/// version's image by name, never the first one it finds.
#[test]
fn release_script_ships_the_disk_image_of_the_version_it_built() {
    let script = repo_file("scripts/release.sh");
    assert!(script.contains(r#"DMG="$BUNDLE_DIR/dmg/Freehold Ledger_${VERSION}_universal.dmg""#), "release.sh does not pick the disk image by version");
    assert!(!script.contains("-name '*.dmg'"), "release.sh still picks any disk image");
}

#[test]
fn agent_guide_keeps_hub_backup_rules_out_of_the_ledger() {
    let guide = repo_file("docs/AGENT-GUIDE.md");
    let section = guide
        .split("## Freehold skills")
        .nth(1)
        .expect("AGENT-GUIDE.md has a Freehold skills section")
        .split("\n## ")
        .next()
        .unwrap();
    assert!(section.contains("freehold-skills/tree/v"), "the skills link must pin a tagged version");
    assert!(!section.contains("backup rules apply"), "freehold-admin's hub backup rules must not be applied wholesale to the Ledger");
    assert!(section.contains("don't apply here"), "the section must say the hub schedule, encryption and pruning rules don't apply");
    assert!(section.contains("never delete the person's backups"), "the section must forbid pruning backups");
}
