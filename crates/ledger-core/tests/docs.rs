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
