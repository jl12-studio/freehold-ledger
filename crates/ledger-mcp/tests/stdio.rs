//! Public-boundary tests: the real `ledger-mcp` binary, spawned as a child process, spoken to
//! over stdio with the rmcp client. Every ledger lives in a per-test temp directory and
//! `FREEHOLD_LEDGER_DB` is set explicitly on the child.

use rmcp::{
    model::{CallToolRequestParams, CallToolResult, ContentBlock},
    service::RunningService,
    transport::{ConfigureCommandExt, TokioChildProcess},
    RoleClient, ServiceExt,
};
use serde_json::{json, Value};
use std::path::Path;
use tokio::process::Command;

type Client = RunningService<RoleClient, ()>;

async fn spawn_server(db: &Path) -> Client {
    let cmd = Command::new(env!("CARGO_BIN_EXE_ledger-mcp")).configure(|c| {
        c.env("FREEHOLD_LEDGER_DB", db);
    });
    ().serve(TokioChildProcess::new(cmd).expect("spawn ledger-mcp"))
        .await
        .expect("MCP handshake")
}

async fn call(client: &Client, tool: &'static str, args: Value) -> CallToolResult {
    let mut params = CallToolRequestParams::new(tool);
    if let Value::Object(map) = args {
        params = params.with_arguments(map);
    }
    client.call_tool(params).await.expect("a tool result, never a protocol error")
}

fn text(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_error(r: &CallToolResult) -> bool {
    r.is_error == Some(true)
}

/// Parse a success result's JSON text content.
fn ok_json(r: &CallToolResult) -> Value {
    assert!(!is_error(r), "expected success, got error: {}", text(r));
    serde_json::from_str(&text(r)).expect("success content is JSON text")
}

const TOOLS: [&str; 29] = [
    "archive_business",
    "assign_payment_method",
    "backup_ledger",
    "correct_transaction",
    "create_business",
    "create_payment_method",
    "export_tax_year",
    "ledger_info",
    "list_backups",
    "list_businesses",
    "list_categories",
    "list_payment_methods",
    "list_transactions",
    "reclassify_transaction",
    "record_expense",
    "record_income",
    "record_transactions",
    "rename_business",
    "rename_payment_method",
    "restore_business",
    "restore_ledger",
    "retire_payment_method",
    "set_default_payment_method",
    "summarize",
    "switch_business",
    "update_business",
    "verify_ledger",
    "void_batch",
    "void_transaction",
];

#[tokio::test]
async fn s_mcp_1_lists_every_tool_with_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;

    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(names, TOOLS);

    for t in &tools {
        let desc = t.description.as_deref().unwrap_or("");
        assert!(!desc.trim().is_empty(), "{} has no description", t.name);
        assert!(!desc.contains('\u{2014}'), "{} description has an em dash", t.name);
        assert_eq!(
            t.input_schema.get("type").and_then(Value::as_str),
            Some("object"),
            "{} input schema must be type object",
            t.name
        );
    }
    client.cancel().await.ok();
}

fn today() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

#[tokio::test]
async fn dollar_amounts_become_exact_cents_and_date_defaults_to_today() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;

    let base = json!({ "description": "Device", "category": "supplies" });
    let with = |amount: Value| {
        let mut a = base.clone();
        a["amount"] = amount;
        a
    };

    let r = ok_json(&call(&client, "record_expense", with(json!("300"))).await);
    assert_eq!(r["amount_cents"], 30000);
    assert_eq!(r["amount"], "$300.00");
    assert_eq!(r["occurred_on"], today());

    let r = ok_json(&call(&client, "record_expense", with(json!("$1,200.5"))).await);
    assert_eq!(r["amount_cents"], 120050);
    assert_eq!(r["amount"], "$1,200.50");

    let r = ok_json(&call(&client, "record_expense", with(json!("300.5"))).await);
    assert_eq!(r["amount_cents"], 30050);

    // A JSON number works too, and is exact to the cent.
    let r = ok_json(&call(&client, "record_expense", with(json!(19.99))).await);
    assert_eq!(r["amount_cents"], 1999);

    client.cancel().await.ok();
}

fn ids(list: &Value) -> Vec<i64> {
    let mut v: Vec<i64> = list["transactions"].as_array().unwrap().iter().map(|t| t["id"].as_i64().unwrap()).collect();
    v.sort();
    v
}

#[tokio::test]
async fn s_mcp_2_scene_and_corrections_via_tools_only() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.sqlite3");
    let client = spawn_server(&db).await;

    // S-REC-1: the scene. $300 device, an asset, no date.
    let device = ok_json(
        &call(
            &client,
            "record_expense",
            json!({ "amount": "300", "description": "Device", "category": "supplies", "is_asset": true }),
        )
        .await,
    );
    assert_eq!(device["amount_cents"], 30000);
    assert_eq!(device["occurred_on"], today());
    assert_eq!(device["is_asset"], true);
    assert!(device["note"].as_str().unwrap().contains("asset"));
    // Recorded with defaults and no questions first: the note must name the stored date and
    // category so the person can see what was assumed and correct it.
    let device_note = device["note"].as_str().unwrap();
    assert!(device_note.contains(&today()), "{device_note}");
    assert!(device_note.contains("Supplies"), "{device_note}");
    let device_id = device["id"].as_i64().unwrap();

    // S-REC-2: income, and a second source spelled differently reuses the first.
    let web = ok_json(
        &call(
            &client,
            "record_income",
            json!({ "amount": 1200, "description": "Website build", "source": "Upwork", "date": "2026-03-10" }),
        )
        .await,
    );
    assert_eq!(web["amount_cents"], 120000);
    assert_eq!(web["category"], "gross_receipts");
    assert_eq!(web["income_source"], "Upwork");
    let web2 = ok_json(
        &call(
            &client,
            "record_income",
            json!({ "amount": "50.25", "description": "Logo tweak", "source": "upwork", "category": "other_income", "date": "2026-04-02" }),
        )
        .await,
    );
    assert_eq!(web2["income_source"], "Upwork", "the existing Upwork source is reused, whatever the case");
    let web_id = web["id"].as_i64().unwrap();
    let web2_id = web2["id"].as_i64().unwrap();

    // S-COR-1: void -> hidden by default, visible with void info when asked; voiding twice is an error.
    let cable = ok_json(
        &call(&client, "record_expense", json!({ "amount": "12", "description": "Cable", "category": "supplies", "date": "2026-03-01" }))
            .await,
    );
    let cable_id = cable["id"].as_i64().unwrap();
    let voided = ok_json(&call(&client, "void_transaction", json!({ "id": cable_id, "reason": "Entered twice" })).await);
    assert_eq!(voided["void"]["reason"], "Entered twice");
    assert!(voided["note"].as_str().unwrap().starts_with("In My business: Voided, not deleted"));
    let visible = ok_json(&call(&client, "list_transactions", json!({})).await);
    assert!(!ids(&visible).contains(&cable_id));
    let all = ok_json(&call(&client, "list_transactions", json!({ "include_voided": true })).await);
    let row = all["transactions"].as_array().unwrap().iter().find(|t| t["id"] == cable_id).unwrap();
    assert_eq!(row["void"]["reason"], "Entered twice");
    let again = call(&client, "void_transaction", json!({ "id": cable_id, "reason": "again" })).await;
    assert!(is_error(&again));
    assert!(text(&again).contains("already voided"), "{}", text(&again));

    // S-COR-2: correct 30.00 -> 300.00 keeps the history.
    let typo = ok_json(
        &call(&client, "record_expense", json!({ "amount": "30", "description": "Monitor", "category": "office_expense", "date": "2026-03-05", "is_asset": true }))
            .await,
    );
    let typo_id = typo["id"].as_i64().unwrap();
    let fixed = ok_json(&call(&client, "correct_transaction", json!({ "id": typo_id, "amount": "300", "reason": "Typo: missing a zero" })).await);
    assert_eq!(fixed["voided"]["id"], typo_id);
    assert_eq!(fixed["voided"]["void"]["reason"], "Typo: missing a zero");
    assert_eq!(fixed["replacement"]["amount_cents"], 30000);
    assert_eq!(fixed["replacement"]["replaces_id"], typo_id);
    assert_eq!(fixed["replacement"]["description"], "Monitor", "untouched fields carry over");
    assert_eq!(fixed["replacement"]["is_asset"], true);
    assert!(fixed["note"].as_str().unwrap().len() > 20);
    let monitor_id = fixed["replacement"]["id"].as_i64().unwrap();

    // S-COR-3: reclassify keeps both categories in the history; a voided row cannot be reclassified.
    let lunch = ok_json(&call(&client, "record_expense", json!({ "amount": "9", "description": "Stamps", "category": "uncategorized", "date": "2026-03-02" })).await);
    let lunch_id = lunch["id"].as_i64().unwrap();
    assert!(lunch["note"].as_str().unwrap().contains("uncategorized"));
    assert!(lunch["note"].as_str().unwrap().contains("2026-03-02"), "the note names the stored date");
    let re = ok_json(&call(&client, "reclassify_transaction", json!({ "id": lunch_id, "category": "office_expense", "reason": "It was postage" })).await);
    assert_eq!(re["category"], "office_expense");
    assert_eq!(re["original_category"], "uncategorized");
    assert_eq!(re["classification_history"][0]["category"], "office_expense");
    let on_voided = call(&client, "reclassify_transaction", json!({ "id": cable_id, "category": "office_expense", "reason": "x" })).await;
    assert!(is_error(&on_voided));
    assert!(text(&on_voided).contains("voided"), "{}", text(&on_voided));

    // S-LST-1: filters return exactly the expected ids (voided originals are hidden by default).
    let by = |args: Value| {
        let c = &client;
        async move { ids(&ok_json(&call(c, "list_transactions", args).await)) }
    };
    assert_eq!(by(json!({ "kind": "income" })).await, vec![web_id, web2_id]);
    assert_eq!(by(json!({ "kind": "income", "source": "UPWORK" })).await, vec![web_id, web2_id]);
    assert_eq!(by(json!({ "category": "other_income" })).await, vec![web2_id]);
    assert_eq!(by(json!({ "is_asset": true })).await, vec![device_id, monitor_id]);
    assert_eq!(by(json!({ "category": "office_expense" })).await, vec![monitor_id, lunch_id]);
    assert_eq!(by(json!({ "from": "2026-03-10", "to": "2026-03-31", "kind": "income" })).await, vec![web_id]);
    let counted = ok_json(&call(&client, "list_transactions", json!({ "kind": "income", "limit": 1 })).await);
    assert_eq!(counted["count"], 1);

    client.cancel().await.ok();

    // The tools wrote real data: the same rows are in the file when ledger-core opens it directly.
    let ledger = ledger_core::Ledger::open(&db).unwrap();
    let rows = ledger.list(&ledger_core::Filter { include_voided: true, ..Default::default() }).unwrap();
    let mut got: Vec<i64> = rows.iter().map(|t| t.id).collect();
    got.sort();
    let mut want = vec![device_id, web_id, web2_id, cable_id, typo_id, monitor_id, lunch_id];
    want.sort();
    assert_eq!(got, want);
    let device_row = ledger.get(device_id).unwrap();
    assert_eq!((device_row.amount_cents, device_row.is_asset), (30000, true));
    assert!(ledger.get(cable_id).unwrap().void.is_some());
    assert_eq!(ledger.get(lunch_id).unwrap().category, "office_expense");
}

#[tokio::test]
async fn s_mcp_3_invalid_input_is_tool_error_and_server_keeps_serving() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let good = json!({ "amount": "10", "description": "Pens", "category": "supplies" });
    let with = |k: &str, v: Value| {
        let mut a = good.clone();
        a[k] = v;
        a
    };

    let cases: Vec<(&'static str, Value, &str)> = vec![
        ("record_expense", with("amount", json!("0")), "more than $0.00"),
        ("record_expense", with("amount", json!("-5")), "more than $0.00"),
        ("record_expense", with("amount", json!("1.234")), "2 decimal places"),
        ("record_expense", with("amount", json!("lots")), "amount"),
        ("record_expense", with("amount", json!(true)), "Could not read the tool input"),
        ("record_expense", with("category", json!("toys")), "toys"),
        ("record_expense", with("date", json!("2026-13-01")), "Dates look like"),
        ("record_expense", json!({ "description": "no amount", "category": "supplies" }), "amount"),
        ("record_income", json!({ "amount": "5", "description": "x", "source": "  " }), "needs a source"),
        ("void_transaction", json!({ "id": 9999, "reason": "nope" }), "Transaction 9999 was not found"),
        ("list_transactions", json!({ "limit": 501 }), "between 1 and 500"),
        ("correct_transaction", json!({ "id": 1, "reason": "nothing to change" }), "Say what to change"),
    ];
    for (tool, args, expect) in cases {
        let r = call(&client, tool, args.clone()).await;
        assert!(is_error(&r), "{tool} {args} should be a tool error, got: {}", text(&r));
        assert!(text(&r).contains(expect), "{tool} {args}: message {:?} lacks {expect:?}", text(&r));
    }

    // No row was written by any of that, and the same connection still serves.
    let listed = ok_json(&call(&client, "list_transactions", json!({ "include_voided": true })).await);
    assert_eq!(listed["count"], 0);
    let ok = ok_json(&call(&client, "record_expense", good).await);
    assert_eq!(ok["amount_cents"], 1000);
    client.cancel().await.ok();
}

#[tokio::test]
async fn unopenable_ledger_reports_error_through_tools() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.sqlite3");
    std::fs::write(&db, b"this is not a sqlite file, just text that must stay untouched").unwrap();
    let client = spawn_server(&db).await;

    let info = call(&client, "ledger_info", json!({})).await;
    assert!(is_error(&info));
    assert!(text(&info).contains("is not a Freehold ledger"), "{}", text(&info));
    let rec = call(&client, "record_expense", json!({ "amount": "1", "description": "x", "category": "supplies" })).await;
    assert!(is_error(&rec));
    assert!(text(&rec).contains("is not a Freehold ledger"));
    client.cancel().await.ok();
    assert_eq!(
        std::fs::read(&db).unwrap(),
        b"this is not a sqlite file, just text that must stay untouched"
    );
}

#[tokio::test]
async fn two_processes_share_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.sqlite3");
    let a = spawn_server(&db).await;
    let b = spawn_server(&db).await;

    let from_a = ok_json(&call(&a, "record_expense", json!({ "amount": "1", "description": "from A", "category": "supplies" })).await);
    let from_b = ok_json(&call(&b, "record_income", json!({ "amount": "2", "description": "from B", "source": "Upwork" })).await);

    for (name, client) in [("A", &a), ("B", &b)] {
        let listed = ok_json(&call(client, "list_transactions", json!({})).await);
        assert_eq!(ids(&listed), vec![from_a["id"].as_i64().unwrap(), from_b["id"].as_i64().unwrap()], "server {name} sees both rows");
    }
    a.cancel().await.ok();
    b.cancel().await.ok();
}

#[tokio::test]
async fn ledger_info_and_categories_describe_the_books() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.sqlite3");
    let client = spawn_server(&db).await;

    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["transaction_count"], 0);
    assert_eq!(info["schema_version"], 4);
    assert!(info["path"].as_str().unwrap().ends_with("ledger.sqlite3"));
    assert!(info["note"].as_str().unwrap().contains("nothing is ever deleted"));

    let income = ok_json(&call(&client, "list_categories", json!({ "kind": "income" })).await);
    let keys: Vec<&str> = income["categories"].as_array().unwrap().iter().map(|c| c["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["gross_receipts", "other_income"]);
    let all = ok_json(&call(&client, "list_categories", json!({})).await);
    let supplies = all["categories"].as_array().unwrap().iter().find(|c| c["key"] == "supplies").unwrap();
    assert_eq!(supplies["schedule_c_line"], "22");
    assert_eq!(supplies["label"], "Supplies");
    assert_eq!(all["categories"].as_array().unwrap().len(), 25);
    client.cancel().await.ok();
}

fn seeded_db(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("ledger.sqlite3");
    let mut l = ledger_core::Ledger::open(&path).unwrap();
    ledger_core::seed_demo(&mut l).unwrap();
    path
}

#[tokio::test]
async fn summarize_tool_answers_in_one_call() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;

    // Named period: resolved against the server's local date.
    let r = call(&client, "summarize", json!({ "period": "last_quarter" })).await;
    let v = ok_json(&r);
    let (from, to) = ledger_core::resolve_period(ledger_core::Period::LastQuarter, chrono::Local::now().date_naive());
    assert_eq!(v["from"], from.to_string());
    assert_eq!(v["to"], to.to_string());
    assert!(v["note"].as_str().is_some_and(|n| !n.is_empty()));
    assert!(v["profit"].as_str().unwrap().starts_with('$') || v["profit"].as_str().unwrap().starts_with("-$"));

    // Explicit range and source: one call, formatted dollars.
    let r = call(&client, "summarize", json!({ "from": "2026-01-01", "to": "2026-12-31", "source": "upwork" })).await;
    let v = ok_json(&r);
    assert_eq!(v["income_total_cents"], 120000);
    assert_eq!(v["income_total"], "$1,200.00");
    assert_eq!(v["by_source"][0]["source"], "Upwork");
    assert_eq!(v["by_source"][0]["total"], "$1,200.00");

    // Equipment: asset flag.
    let r = call(&client, "summarize", json!({ "from": "2026-10-01", "to": "2026-12-31", "is_asset": true })).await;
    let v = ok_json(&r);
    assert_eq!(v["expenses_total"], "$300.00");
    assert_eq!(v["assets"]["count"], 1);
    client.cancel().await.ok();
}

#[tokio::test]
async fn summarize_requires_period_or_range() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    for args in [
        json!({}),
        json!({ "from": "2026-01-01" }),
        json!({ "period": "this_year", "from": "2026-01-01", "to": "2026-02-01" }),
    ] {
        let r = call(&client, "summarize", args).await;
        assert!(is_error(&r));
        assert!(text(&r).contains("period"), "{}", text(&r));
    }
    let r = call(&client, "summarize", json!({ "from": "2026-03-01", "to": "2026-01-01" })).await;
    assert!(is_error(&r));
    let r = call(&client, "summarize", json!({ "period": "next_decade" })).await;
    assert!(is_error(&r));
    client.cancel().await.ok();
}

#[tokio::test]
async fn export_tool_writes_folder() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    let r = call(&client, "export_tax_year", json!({ "year": 2026, "destination": out.path() })).await;
    let v = ok_json(&r);
    let folder = std::path::PathBuf::from(v["folder"].as_str().unwrap());
    assert_eq!(folder, out.path().join("Freehold Ledger My business 2026 export"));
    assert_eq!(v["files"].as_array().unwrap().len(), 7);
    assert!(folder.join("README.txt").is_file());
    assert_eq!(v["totals"]["expenses_total_cents"], 68709);
    let note = v["note"].as_str().unwrap();
    assert!(note.contains("Your 2026 books are in") && note.contains("README.txt"), "{note}");
    client.cancel().await.ok();
}

#[tokio::test]
async fn export_tool_rejects_relative_destination() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    let r = call(&client, "export_tax_year", json!({ "year": 2026, "destination": "exports/here" })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("full folder path"), "{}", text(&r));
    client.cancel().await.ok();
}

#[tokio::test]
async fn unknown_category_filter_is_a_tool_error_not_an_empty_answer() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    for (tool, args) in [
        ("summarize", json!({ "from": "2026-01-01", "to": "2026-12-31", "category": "software" })),
        ("list_transactions", json!({ "category": "software" })),
    ] {
        let r = call(&client, tool, args).await;
        assert!(is_error(&r), "{tool} accepted an unknown category: {}", text(&r));
        assert!(text(&r).contains("\"software\" is not a category key") && text(&r).contains("office_expense"), "{}", text(&r));
    }
    // A real key still filters.
    let v = ok_json(&call(&client, "summarize", json!({ "from": "2026-01-01", "to": "2026-12-31", "category": "supplies" })).await);
    assert_eq!(v["expenses_total_cents"], 60000);
    client.cancel().await.ok();
}

fn gd(date: &str, amount: &str) -> Value {
    json!({ "kind": "expense", "amount": amount, "description": "GODADDY.COM", "date": date, "category": "other_expense" })
}

#[tokio::test]
async fn record_transactions_skips_reimported_rows() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let staples = json!({ "kind": "expense", "amount": "86.40", "description": "STAPLES #0412", "date": "2026-01-06", "category": "supplies" });
    let args = json!({
        "label": "bank-export.csv",
        "entries": [gd("2026-01-20", "18.17"), gd("2026-01-20", "18.17"), staples],
    });
    let first = ok_json(&call(&client, "record_transactions", args.clone()).await);
    assert_eq!(first["recorded_count"], 2);
    assert_eq!(first["skipped_count"], 1);
    assert_eq!(first["skipped_duplicates"][0]["entry"], 2);
    assert!(first["batch_id"].is_number());
    assert!(first["note"].as_str().unwrap().contains("allow_duplicate"));

    let again = ok_json(&call(&client, "record_transactions", args).await);
    assert_eq!(again["recorded_count"], 0);
    assert!(again["batch_id"].is_null());
    assert!(again["note"].as_str().unwrap().contains("Nothing new was recorded"));

    let listed = ok_json(&call(&client, "list_transactions", json!({})).await);
    assert_eq!(listed["count"], 2);
}

#[tokio::test]
async fn void_batch_undoes_an_import() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let staples = json!({ "kind": "expense", "amount": "86.40", "description": "STAPLES #0412", "date": "2026-01-06", "category": "supplies" });
    let lic = json!({ "kind": "expense", "amount": "75.00", "description": "CITY OF AUSTIN BUSINESS LICENSE", "date": "2026-01-22", "category": "taxes_licenses" });
    let rec = ok_json(&call(&client, "record_transactions", json!({
        "label": "bank-export.csv",
        "entries": [gd("2026-01-20", "18.17"), staples, lic],
    })).await);
    assert!(rec["note"].as_str().unwrap().contains("void_batch"));
    let bid = rec["batch_id"].as_i64().unwrap();

    let v = ok_json(&call(&client, "void_batch", json!({ "batch_id": bid, "reason": "Imported the wrong file" })).await);
    assert_eq!(v["voided_count"], 3);
    assert_eq!(v["already_voided_count"], 0);
    assert!(v["note"].as_str().unwrap().contains("Voided 3 entries"));

    let live = ok_json(&call(&client, "list_transactions", json!({ "batch_id": bid })).await);
    assert_eq!(live["count"], 0);
    let all = ok_json(&call(&client, "list_transactions", json!({ "batch_id": bid, "include_voided": true })).await);
    assert_eq!(all["count"], 3);

    let s = ok_json(&call(&client, "summarize", json!({ "from": "2026-01-01", "to": "2026-12-31" })).await);
    assert_eq!(s["expenses_total_cents"], 0);

    let again = ok_json(&call(&client, "void_batch", json!({ "batch_id": bid, "reason": "again" })).await);
    assert_eq!(again["voided_count"], 0);
    assert!(again["note"].as_str().unwrap().contains("Nothing left to void"));

    let unknown = call(&client, "void_batch", json!({ "batch_id": 999, "reason": "x" })).await;
    assert!(is_error(&unknown));
    assert!(text(&unknown).contains("Batch 999 was not found"));
    let missing = call(&client, "void_batch", json!({ "batch_id": bid })).await;
    assert!(is_error(&missing));
    client.cancel().await.ok();
}

#[tokio::test]
async fn record_transactions_bad_entry_is_tool_error() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;

    let r = call(&client, "record_transactions", json!({
        "label": "x", "entries": [gd("2026-01-20", "18.17"), gd("2026-01-21", "-24.99")],
    })).await;
    assert!(is_error(&r));
    let t = text(&r);
    assert!(t.contains("Entry 2"), "{t}");
    assert!(t.contains("correct_transaction"), "{t}");
    assert!(t.ends_with("Nothing in this batch was recorded."), "{t}");
    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["transaction_count"], 0);

    let many: Vec<Value> = (0..51).map(|i| gd("2026-01-05", &format!("{}.00", i + 1))).collect();
    let r = call(&client, "record_transactions", json!({ "label": "x", "entries": many })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("50"));

    let mut unknown = gd("2026-01-20", "1.00");
    unknown["bogus"] = json!(1);
    let r = call(&client, "record_transactions", json!({ "label": "x", "entries": [unknown] })).await;
    assert!(is_error(&r));

    let r = call(&client, "record_transactions", json!({
        "label": "x",
        "entries": [{ "kind": "expense", "amount": "5", "description": "no cat", "date": "2026-01-20" }],
    })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("Entry 1"), "{}", text(&r));

    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["transaction_count"], 0);
}

#[tokio::test]
async fn s_pan_7_no_tool_accepts_a_full_card_number() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let card = "4242 4242 4242 4242";
    let cases = [
        ("record_expense", json!({ "amount": "12", "description": format!("Visa {card} exp 12/27"), "category": "supplies" }), "description"),
        ("record_income", json!({ "amount": "50", "description": "Fee", "source": "Acct 000123456789012" }), "source"),
        ("void_transaction", json!({ "id": "4242424242424242", "reason": "x" }), "id"),
        ("record_transactions", json!({ "label": "jan.csv", "entries": [
            { "kind": "expense", "amount": "5", "description": "Coffee", "date": "2026-01-02", "category": "meals" },
            { "kind": "expense", "amount": "6", "description": format!("Card {card}"), "date": "2026-01-03", "category": "meals" }
        ] }), "description of entry 2"),
        ("export_tax_year", json!({ "year": 2026, "destination": "/tmp/4242424242424242" }), "destination"),
        ("list_transactions", json!({ "source": "4242-4242-4242-4242" }), "source"),
    ];
    for (tool, args, field) in cases {
        let r = call(&client, tool, args).await;
        assert!(is_error(&r), "{tool} accepted a card number");
        let t = text(&r);
        assert!(t.starts_with(&format!("The {field} ")), "{tool}: {t}");
        assert!(t.contains("last 4 digits") && t.contains("nothing was stored"), "{tool}: {t}");
        assert!(!t.contains("4242") && !t.contains("0001234"), "{tool} echoed the number: {t}");
    }
    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["transaction_count"], 0);
    // Ordinary numbers still pass.
    let ok = ok_json(&call(&client, "record_expense", json!({ "amount": "$1,200.50", "description": "Invoice 123456789012 from 2026-01-04", "category": "supplies" })).await);
    assert_eq!(ok["amount_cents"], 120050);
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_pan_7_full_number_as_a_field_name_is_refused_without_echo() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let cases = [
        ("record_expense", json!({ "amount": "12", "description": "Pens", "category": "supplies", "4242424242424242": "x" })),
        ("record_expense", json!({ "amount": "12", "description": "Pens", "category": "supplies", "4242 4242 4242 4242": "4111111111111111" })),
        ("record_transactions", json!({ "label": "jan.csv", "entries": [
            { "kind": "expense", "amount": "5", "description": "Coffee", "date": "2026-01-02", "category": "meals", "4242-4242-4242-4242": true }
        ] })),
        ("create_payment_method", json!({ "kind": "card", "nickname": "Visa", "4242424242424242": null })),
    ];
    for (tool, args) in cases {
        let r = call(&client, tool, args).await;
        assert!(is_error(&r), "{tool} accepted a card number as a field name");
        let t = text(&r);
        assert!(t.contains("last 4 digits") && t.contains("nothing was stored"), "{tool}: {t}");
        assert!(!t.contains("4242") && !t.contains("4111"), "{tool} echoed the number: {t}");
    }
    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["transaction_count"], 0);
    client.cancel().await.ok();
}

fn note(v: &Value) -> String {
    let n = v["note"].as_str().expect("note is text").to_string();
    assert!(!n.contains('\u{2014}'), "note has an em dash: {n}");
    n
}

#[tokio::test]
async fn s_mcp_7_card_by_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;

    let v = ok_json(&call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "Business Visa", "last4": "4242", "make_default": true })).await);
    assert_eq!(v["is_default"], true);
    let n = note(&v);
    assert!(n.contains("Business Visa (card ending 4242)"), "{n}");
    assert!(n.contains("never a full number"), "{n}");

    let v = ok_json(&call(&client, "create_payment_method", json!({ "kind": "cash", "nickname": "Cash" })).await);
    assert!(note(&v).contains("Business Visa stays the default"));

    let v = ok_json(&call(&client, "list_payment_methods", json!({})).await);
    assert_eq!(v["count"], 2);
    assert_eq!(v["payment_methods"][0]["nickname"], "Business Visa");
    let n = note(&v);
    assert!(n.contains("Business Visa"), "{n}");
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_8_payment_method_refusals() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "Business Visa", "last4": "4242", "make_default": true })).await);
    ok_json(&call(&client, "create_payment_method", json!({ "kind": "cash", "nickname": "Cash" })).await);

    let r = call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "X", "last4": "4242 4242 4242 4242" })).await;
    assert!(is_error(&r));
    let t = text(&r);
    assert!(t.contains("last 4 digits") && t.contains("nothing was stored"), "{t}");
    assert!(!t.contains("4242 4242"), "{t}");

    let r = call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "X", "last4": "42424" })).await;
    assert!(is_error(&r) && text(&r).contains("exactly 4 digits"), "{}", text(&r));

    for field in ["card_number", "expiry", "cvv"] {
        let r = call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "X", field: "1" })).await;
        assert!(is_error(&r), "{field} must be refused");
    }

    let r = call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "business visa" })).await;
    assert!(is_error(&r) && text(&r).contains("already in use"), "{}", text(&r));

    let r = call(&client, "create_payment_method", json!({ "kind": "cash", "nickname": "Petty", "last4": "1234" })).await;
    assert!(is_error(&r));

    let v = ok_json(&call(&client, "list_payment_methods", json!({})).await);
    assert_eq!(v["count"], 2);
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_9_rename_retire_default() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let first = ok_json(&call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "Business Visa", "last4": "4242", "make_default": true })).await);
    ok_json(&call(&client, "create_payment_method", json!({ "kind": "cash", "nickname": "Cash" })).await);

    let v = ok_json(&call(&client, "rename_payment_method", json!({ "payment_method": "business visa", "nickname": "Biz Visa" })).await);
    assert!(note(&v).contains("Renamed to Biz Visa"));
    let id = first["id"].clone();
    let v = ok_json(&call(&client, "rename_payment_method", json!({ "payment_method": id, "nickname": "Biz Visa 2" })).await);
    assert!(note(&v).contains("Renamed to Biz Visa 2"));
    ok_json(&call(&client, "rename_payment_method", json!({ "payment_method": "Biz Visa 2", "nickname": "Biz Visa" })).await);

    let v = ok_json(&call(&client, "retire_payment_method", json!({ "payment_method": "Biz Visa" })).await);
    let n = note(&v);
    assert!(n.contains("retired, not deleted") && n.contains("no default now"), "{n}");

    let v = ok_json(&call(&client, "list_payment_methods", json!({})).await);
    assert_eq!(v["count"], 1);
    let v = ok_json(&call(&client, "list_payment_methods", json!({ "include_retired": true })).await);
    assert_eq!(v["count"], 2);
    assert!(v["payment_methods"].as_array().unwrap().iter().any(|p| p["retired"] == true));

    let r = call(&client, "set_default_payment_method", json!({ "payment_method": "Biz Visa" })).await;
    assert!(is_error(&r) && text(&r).contains("retired"), "{}", text(&r));

    let v = ok_json(&call(&client, "set_default_payment_method", json!({ "payment_method": "Cash" })).await);
    assert!(note(&v).contains("Cash"));
    let v = ok_json(&call(&client, "set_default_payment_method", json!({ "clear": true })).await);
    assert!(note(&v).contains("No default now"));

    assert!(is_error(&call(&client, "set_default_payment_method", json!({})).await));
    assert!(is_error(&call(&client, "set_default_payment_method", json!({ "payment_method": "Cash", "clear": true })).await));
    let r = call(&client, "set_default_payment_method", json!({ "payment_method": "Nope" })).await;
    assert!(is_error(&r) && text(&r).contains("list_payment_methods"), "{}", text(&r));
    client.cancel().await.ok();
}

async fn pm_setup(client: &Client) {
    ok_json(&call(client, "create_payment_method", json!({ "kind": "card", "nickname": "Business Visa", "last4": "4242", "make_default": true })).await);
    ok_json(&call(client, "create_payment_method", json!({ "kind": "bank_account", "nickname": "Checking", "last4": "6789" })).await);
    ok_json(&call(client, "create_payment_method", json!({ "kind": "cash", "nickname": "Cash" })).await);
}

fn txn_ids(v: &Value) -> Vec<i64> {
    v["transactions"].as_array().unwrap().iter().map(|t| t["id"].as_i64().unwrap()).collect()
}

#[tokio::test]
async fn s_mcp_11_assign_one_and_many() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    pm_setup(&client).await;

    let v = ok_json(&call(&client, "assign_payment_method", json!({ "payment_method": "business visa", "ids": [1] })).await);
    assert_eq!(v["assigned_count"], 1);
    let n = note(&v);
    assert!(n.contains("Business Visa (card ending 4242)"), "{n}");

    let v = ok_json(&call(&client, "assign_payment_method", json!({ "payment_method": "Checking", "ids": [2, 3] })).await);
    assert_eq!(v["assigned_count"], 2);
    let v = ok_json(&call(&client, "assign_payment_method", json!({ "payment_method": "Checking", "ids": [2, 3] })).await);
    assert_eq!(v["assigned_count"], 0);
    assert_eq!(v["unchanged_count"], 2);
    assert!(note(&v).starts_with("In My business: Nothing changed"), "{}", note(&v));

    let v = ok_json(&call(&client, "list_transactions", json!({ "payment_method": "Checking" })).await);
    let mut got = txn_ids(&v);
    got.sort();
    assert_eq!(got, vec![2, 3]);
    let v = ok_json(&call(&client, "list_transactions", json!({ "payment_method": "none" })).await);
    assert_eq!(txn_ids(&v), vec![10, 4, 5, 8, 9]);
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_12_assign_refusals() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    pm_setup(&client).await;

    let r = call(&client, "assign_payment_method", json!({ "payment_method": "Cash", "ids": [4, 6] })).await;
    assert!(is_error(&r));
    let t = text(&r);
    assert!(t.contains("voided") && t.contains("Nothing was assigned"), "{t}");
    let v = ok_json(&call(&client, "list_transactions", json!({ "payment_method": "none" })).await);
    assert!(txn_ids(&v).contains(&4));

    assert!(is_error(&call(&client, "assign_payment_method", json!({ "payment_method": "Cash", "ids": [] })).await));
    let many: Vec<i64> = (1..=201).collect();
    let r = call(&client, "assign_payment_method", json!({ "payment_method": "Cash", "ids": many })).await;
    assert!(is_error(&r) && text(&r).contains("200"), "{}", text(&r));
    let r = call(&client, "assign_payment_method", json!({ "payment_method": "Nope", "ids": [1] })).await;
    assert!(is_error(&r) && text(&r).contains("list_payment_methods"), "{}", text(&r));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_13_default_on_record() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    pm_setup(&client).await;

    let v = ok_json(&call(&client, "record_expense", json!({ "amount": "40", "description": "Mouse", "category": "supplies" })).await);
    assert_eq!(v["payment_method"]["nickname"], "Business Visa");
    let n = note(&v);
    assert!(n.contains("(your default)"), "{n}");

    let v = ok_json(&call(&client, "record_expense", json!({ "amount": "5", "description": "Pen", "category": "supplies", "payment_method": "cash" })).await);
    assert_eq!(v["payment_method"]["nickname"], "Cash");

    let v = ok_json(&call(&client, "record_income", json!({ "amount": "500", "description": "Logo", "source": "Acme" })).await);
    assert!(v["payment_method"].is_null());
    assert!(note(&v).contains("No payment method is set"), "{}", note(&v));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_14_correction_keeps_method() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    pm_setup(&client).await;
    let m = ok_json(&call(&client, "record_expense", json!({ "amount": "40", "description": "Mouse", "category": "supplies" })).await);
    let id = m["id"].as_i64().unwrap();

    let v = ok_json(&call(&client, "correct_transaction", json!({ "id": id, "reason": "wrong amount", "amount": "45" })).await);
    assert_eq!(v["replacement"]["payment_method"]["nickname"], "Business Visa");
    assert!(note(&v).contains("(kept from the original)"), "{}", note(&v));
    let id2 = v["replacement"]["id"].as_i64().unwrap();

    let v = ok_json(&call(&client, "correct_transaction", json!({ "id": id2, "reason": "was cash", "payment_method": "Cash" })).await);
    assert_eq!(v["replacement"]["payment_method"]["nickname"], "Cash");
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_15_batch_payment_methods() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    pm_setup(&client).await;
    let entries = json!([
        { "kind": "expense", "amount": "18.17", "description": "GODADDY.COM G1", "date": "2026-01-05", "category": "advertising" },
        { "kind": "income", "amount": "200", "description": "Upwork payout UP", "date": "2026-01-06", "source": "Upwork" },
        { "kind": "expense", "amount": "9.99", "description": "Stationery ST", "date": "2026-01-07", "category": "supplies", "payment_method": "Cash" }
    ]);
    let v = ok_json(&call(&client, "record_transactions", json!({ "label": "chase-jan.csv", "payment_method": "Checking", "entries": entries })).await);
    let rec = v["recorded"].as_array().unwrap();
    assert_eq!(rec[0]["payment_method"]["nickname"], "Checking");
    assert_eq!(rec[1]["payment_method"]["nickname"], "Checking");
    assert_eq!(rec[2]["payment_method"]["nickname"], "Cash");
    assert!(note(&v).contains("Payment methods: Checking on 2, Cash on 1"), "{}", note(&v));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_16_summarize_by_payment_method() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&seeded_db(dir.path())).await;
    pm_setup(&client).await;
    ok_json(&call(&client, "assign_payment_method", json!({ "payment_method": "business visa", "ids": [1] })).await);
    ok_json(&call(&client, "assign_payment_method", json!({ "payment_method": "Checking", "ids": [2, 3] })).await);

    let v = ok_json(&call(&client, "summarize", json!({ "from": "2026-01-01", "to": "2026-12-31" })).await);
    assert_eq!(v["by_payment_method"][0]["nickname"], "Checking");
    assert_eq!(v["by_payment_method"][0]["income_total"], "$1,285.00");
    let v = ok_json(&call(&client, "summarize", json!({ "from": "2026-01-01", "to": "2026-12-31", "payment_method": "Business Visa" })).await);
    assert_eq!(v["expenses_total"], "$300.00");
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_17_rename_letter_case() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "Biz Visa", "last4": "4242" })).await);

    // S-REN-4: a letter-case-only rename is stored and the note says so.
    let v = ok_json(&call(&client, "rename_payment_method", json!({ "payment_method": "biz visa", "nickname": "BIZ VISA" })).await);
    assert_eq!(v["nickname"], "BIZ VISA");
    let n = note(&v);
    assert!(n.contains("Renamed to BIZ VISA"), "{n}");
    let v = ok_json(&call(&client, "list_payment_methods", json!({})).await);
    assert_eq!(v["payment_methods"][0]["nickname"], "BIZ VISA");

    // S-REN-5: the identical name changes nothing and the note does not claim a rename.
    let v = ok_json(&call(&client, "rename_payment_method", json!({ "payment_method": "BIZ VISA", "nickname": "BIZ VISA" })).await);
    assert_eq!(v["nickname"], "BIZ VISA");
    let n = note(&v);
    assert!(n.contains("already has that name") && n.contains("nothing changed"), "{n}");
    assert!(!n.contains("Renamed"), "{n}");
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_17_business_tools_are_listed_with_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 29);
    let fields = |name: &str| -> Vec<String> {
        let t = tools.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{name} missing"));
        let mut f: Vec<String> = t
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        f.sort();
        f
    };
    assert_eq!(fields("create_business"), ["is_test", "name", "note", "switch_to"]);
    assert_eq!(fields("list_businesses"), ["include_archived"]);
    assert_eq!(fields("switch_business"), ["business"]);
    assert_eq!(fields("rename_business"), ["business", "name"]);
    assert_eq!(fields("update_business"), ["business", "clear_note", "is_test", "note"]);
    assert_eq!(fields("archive_business"), ["business"]);
    assert_eq!(fields("restore_business"), ["business"]);
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_18_create_a_test_business_then_switch() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let v = ok_json(&call(&client, "create_business", json!({ "name": "Test shop", "is_test": true })).await);
    assert_eq!(v["is_test"], true);
    assert_eq!(v["is_current"], false);
    assert_eq!(
        note(&v),
        "Created Test shop (a test business). New entries still go to My business; say \"switch to Test shop\" to record there."
    );
    let v = ok_json(&call(&client, "switch_business", json!({ "business": "test shop" })).await);
    assert_eq!(note(&v), "New entries now go to Test shop (test business).");
    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["current_business"]["name"], "Test shop");
    assert_eq!(info["businesses"].as_array().unwrap().len(), 2);
    assert!(note(&info).contains("in 2 businesses; new entries go to Test shop"), "{}", note(&info));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_19_business_refusals() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_business", json!({ "name": "Test shop" })).await);

    let r = call(&client, "create_business", json!({ "name": "test SHOP" })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("A business called \"Test shop\" already exists"), "{}", text(&r));

    let r = call(&client, "create_business", json!({ "name": "Acct 4242 4242 4242 4242" })).await;
    assert!(is_error(&r));
    assert!(!text(&r).contains("4242"), "{}", text(&r));

    let r = call(&client, "switch_business", json!({ "business": "Nope" })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("No business matches \"Nope\". Call list_businesses"), "{}", text(&r));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_20_rename_and_update_business() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_business", json!({ "name": "Test shop", "is_test": true })).await);

    let v = ok_json(&call(&client, "rename_business", json!({ "business": "Test shop", "name": "Trial shop" })).await);
    assert_eq!(note(&v), "Renamed Test shop to Trial shop.");
    let v = ok_json(&call(&client, "rename_business", json!({ "business": "Trial shop", "name": "Trial shop" })).await);
    assert_eq!(note(&v), "Trial shop already has that name.");

    let v = ok_json(&call(&client, "update_business", json!({ "business": "Trial shop", "note": "Scratch space" })).await);
    assert_eq!(note(&v), "Updated Trial shop: note set.");
    let v = ok_json(&call(&client, "update_business", json!({ "business": "Trial shop", "clear_note": true })).await);
    assert_eq!(note(&v), "Updated Trial shop: note cleared.");
    let v = ok_json(&call(&client, "list_businesses", json!({})).await);
    assert!(v["businesses"].as_array().unwrap().iter().all(|b| b["note"].is_null()));
    let v = ok_json(&call(&client, "update_business", json!({ "business": "Trial shop", "is_test": false })).await);
    assert_eq!(v["is_test"], false);
    assert_eq!(note(&v), "Updated Trial shop: marked as a real business.");
    let v = ok_json(&call(&client, "update_business", json!({ "business": 2, "is_test": true })).await);
    assert_eq!(note(&v), "Updated Trial shop: marked as a test business.");

    let r = call(&client, "update_business", json!({ "business": "Trial shop" })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("Give a note, clear_note or is_test to change."), "{}", text(&r));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_21_archive_and_restore_business() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_business", json!({ "name": "Test shop", "is_test": true, "switch_to": true })).await);

    let v = ok_json(&call(&client, "archive_business", json!({ "business": "Test shop" })).await);
    assert_eq!(
        note(&v),
        "Archived Test shop; nothing was deleted. No business is current now: say which one to record in."
    );
    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert!(info["current_business"].is_null());
    assert!(note(&info).contains("switch_business"), "{}", note(&info));

    let v = ok_json(&call(&client, "list_businesses", json!({})).await);
    assert_eq!(v["count"], 1);
    assert!(note(&v).contains("None is current: call switch_business"), "{}", note(&v));
    let v = ok_json(&call(&client, "list_businesses", json!({ "include_archived": true })).await);
    assert_eq!(v["count"], 2);

    let v = ok_json(&call(&client, "restore_business", json!({ "business": "Test shop" })).await);
    assert_eq!(note(&v), "Restored Test shop.");
    assert_eq!(v["is_current"], false);
    assert_eq!(v["archived"], false);
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_27_business_replies_carry_the_stored_note_as_business_note() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let v = ok_json(&call(&client, "create_business", json!({ "name": "Test shop" })).await);
    assert!(v["business_note"].is_null());
    assert!(note(&v).starts_with("Created Test shop"), "{}", note(&v));

    let v = ok_json(&call(&client, "update_business", json!({ "business": "Test shop", "note": "Scratch space" })).await);
    assert_eq!(note(&v), "Updated Test shop: note set.");
    assert_eq!(v["business_note"], "Scratch space");
    for (tool, args) in [
        ("switch_business", json!({ "business": "Test shop" })),
        ("archive_business", json!({ "business": "Test shop" })),
        ("restore_business", json!({ "business": "Test shop" })),
        ("rename_business", json!({ "business": "Test shop", "name": "Trial shop" })),
    ] {
        let v = ok_json(&call(&client, tool, args).await);
        assert_eq!(v["business_note"], "Scratch space", "{tool}");
    }
    let v = ok_json(&call(&client, "list_businesses", json!({})).await);
    assert!(v["businesses"].as_array().unwrap().iter().any(|b| b["note"] == "Scratch space"));

    let v = ok_json(&call(&client, "update_business", json!({ "business": "Trial shop", "clear_note": true })).await);
    assert_eq!(note(&v), "Updated Trial shop: note cleared.");
    assert!(v["business_note"].is_null());
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_22_recording_names_the_current_business() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let v = ok_json(&call(&client, "record_expense", json!({ "amount": "300", "description": "Device", "category": "supplies" })).await);
    assert_eq!(v["business"]["name"], "My business");
    assert_eq!(v["business"]["is_test"], false);
    assert!(v["business"]["id"].is_i64());
    assert!(note(&v).starts_with("In My business: Recorded on"), "{}", note(&v));

    let v = ok_json(&call(&client, "summarize", json!({ "period": "this_year" })).await);
    assert_eq!(v["business"]["name"], "My business");
    assert!(note(&v).starts_with("In My business: "), "{}", note(&v));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_23_one_call_in_another_business_without_switching() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_business", json!({ "name": "Test shop", "is_test": true })).await);
    let mine = ok_json(&call(&client, "record_expense", json!({ "amount": "10", "description": "Mine", "category": "supplies" })).await);
    let v = ok_json(&call(&client, "record_expense", json!({ "amount": "20", "description": "Theirs", "category": "supplies", "business": "Test shop" })).await);
    assert_eq!(v["business"]["name"], "Test shop");
    assert_eq!(v["business"]["is_test"], true);
    assert!(note(&v).starts_with("In Test shop (test business): "), "{}", note(&v));
    let theirs_id = v["id"].as_i64().unwrap();

    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["current_business"]["name"], "My business");

    let l = ok_json(&call(&client, "list_transactions", json!({})).await);
    assert_eq!(txn_ids(&l), vec![mine["id"].as_i64().unwrap()]);
    assert_eq!(l["business"]["name"], "My business");
    let l = ok_json(&call(&client, "list_transactions", json!({ "business": "Test shop" })).await);
    assert_eq!(txn_ids(&l), vec![theirs_id]);
    assert_eq!(l["business"]["name"], "Test shop");

    let b = ok_json(&call(&client, "record_transactions", json!({ "label": "f.csv", "business": "Test shop", "entries": [{ "kind": "expense", "amount": "5", "description": "Row", "date": "2026-03-01", "category": "supplies" }] })).await);
    assert_eq!(b["business"]["name"], "Test shop");
    assert!(note(&b).starts_with("In Test shop (test business): "), "{}", note(&b));
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_24_no_current_or_archived_business_records_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "archive_business", json!({ "business": "My business" })).await);

    let r = call(&client, "record_income", json!({ "amount": "100", "description": "Job", "source": "Upwork" })).await;
    assert!(is_error(&r));
    assert_eq!(text(&r), ledger_core::LedgerError::NoCurrentBusiness.to_string());

    let r = call(&client, "record_income", json!({ "amount": "100", "description": "Job", "source": "Upwork", "business": "My business" })).await;
    assert!(is_error(&r));
    assert_eq!(text(&r), ledger_core::LedgerError::ArchivedBusiness("My business".into()).to_string());

    let l = ok_json(&call(&client, "list_transactions", json!({ "business": "My business" })).await);
    assert_eq!(l["count"], 0);
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_25_id_based_tools_name_the_entrys_business() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_business", json!({ "name": "Test shop", "is_test": true })).await);
    let e = ok_json(&call(&client, "record_expense", json!({ "amount": "20", "description": "Theirs", "category": "supplies", "business": "Test shop" })).await);
    let id = e["id"].as_i64().unwrap();

    let v = ok_json(&call(&client, "reclassify_transaction", json!({ "id": id, "category": "travel", "reason": "Trip" })).await);
    assert_eq!(v["business"]["name"], "Test shop");
    assert!(note(&v).starts_with("In Test shop (test business): Reclassified"), "{}", note(&v));
    let v = ok_json(&call(&client, "correct_transaction", json!({ "id": id, "reason": "Typo", "amount": "25" })).await);
    assert!(note(&v).starts_with("In Test shop (test business): Corrected"), "{}", note(&v));
    let new_id = v["replacement"]["id"].as_i64().unwrap();
    let v = ok_json(&call(&client, "void_transaction", json!({ "id": new_id, "reason": "Mistake" })).await);
    assert!(note(&v).starts_with("In Test shop (test business): Voided, not deleted"), "{}", note(&v));
    assert_eq!(v["business"]["is_test"], true);

    let b = ok_json(&call(&client, "record_transactions", json!({ "label": "f.csv", "business": "Test shop", "entries": [{ "kind": "expense", "amount": "5", "description": "Row", "date": "2026-03-01", "category": "supplies" }] })).await);
    let v = ok_json(&call(&client, "void_batch", json!({ "batch_id": b["batch_id"], "reason": "Wrong file" })).await);
    assert!(note(&v).starts_with("In Test shop (test business): Voided 1 entries"), "{}", note(&v));
    assert_eq!(v["business"]["name"], "Test shop");
    client.cancel().await.ok();
}

#[tokio::test]
async fn s_mcp_26_payment_methods_and_export_follow_the_named_business() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    ok_json(&call(&client, "create_business", json!({ "name": "Test shop", "is_test": true })).await);
    let v = ok_json(&call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "Business Visa", "last4": "1111" })).await);
    assert_eq!(v["business"]["name"], "My business");
    let first_id = v["id"].as_i64().unwrap();

    let v = ok_json(&call(&client, "create_payment_method", json!({ "kind": "card", "nickname": "Business Visa", "last4": "1111", "business": "Test shop" })).await);
    assert_eq!(v["business"]["name"], "Test shop");
    assert!(note(&v).starts_with("In Test shop (test business): Saved"), "{}", note(&v));

    let v = ok_json(&call(&client, "list_payment_methods", json!({ "business": "Test shop" })).await);
    assert_eq!(v["count"], 1);
    assert_ne!(v["payment_methods"][0]["id"].as_i64().unwrap(), first_id);
    assert_eq!(v["business"]["name"], "Test shop");

    let e = ok_json(&call(&client, "record_expense", json!({ "amount": "20", "description": "Theirs", "category": "supplies", "date": "2026-03-01", "business": "Test shop" })).await);
    let r = call(&client, "assign_payment_method", json!({ "payment_method": first_id, "ids": [e["id"]], "business": "Test shop" })).await;
    assert!(is_error(&r));
    assert_eq!(
        text(&r),
        ledger_core::LedgerError::PaymentMethodInOtherBusiness { nickname: "Business Visa".into(), business: "My business".into() }.to_string()
    );

    let v = ok_json(&call(&client, "rename_payment_method", json!({ "payment_method": "Business Visa", "nickname": "Shop Visa", "business": "Test shop" })).await);
    assert!(note(&v).starts_with("In Test shop (test business): "), "{}", note(&v));
    let v = ok_json(&call(&client, "set_default_payment_method", json!({ "payment_method": "Shop Visa", "business": "Test shop" })).await);
    assert!(note(&v).starts_with("In Test shop (test business): "), "{}", note(&v));
    let v = ok_json(&call(&client, "retire_payment_method", json!({ "payment_method": "Shop Visa", "business": "Test shop" })).await);
    assert!(note(&v).starts_with("In Test shop (test business): "), "{}", note(&v));

    let dest = tempfile::tempdir().unwrap();
    let v = ok_json(&call(&client, "export_tax_year", json!({ "year": 2026, "destination": dest.path().to_str().unwrap(), "business": "Test shop" })).await);
    assert!(v["folder"].as_str().unwrap().ends_with("Freehold Ledger Test shop (test) 2026 export"), "{}", v["folder"]);
    assert!(note(&v).starts_with("In Test shop (test business): "), "{}", note(&v));
    client.cancel().await.ok();
}

async fn expense(client: &Client, description: &str) {
    ok_json(&call(client, "record_expense", json!({ "amount": "10", "description": description, "category": "supplies", "date": "2026-03-01" })).await);
}

async fn listed_descriptions(client: &Client) -> Vec<String> {
    let v = ok_json(&call(client, "list_transactions", json!({})).await);
    v["transactions"].as_array().unwrap().iter().map(|t| t["description"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn admin_backup_lands_beside_the_ledger_and_verifies() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.sqlite3");
    let client = spawn_server(&db).await;
    expense(&client, "First").await;

    let v = ok_json(&call(&client, "backup_ledger", json!({})).await);
    let path = std::path::PathBuf::from(v["path"].as_str().unwrap());
    assert!(path.is_file());
    assert_eq!(path.parent().unwrap(), dir.path().join("backups"));
    assert_eq!(v["transaction_count"], 1);
    assert!(v["bytes"].as_u64().unwrap() > 0);
    assert!(v["schema_version"].is_number() && v["created_at"].is_string());
    let n = note(&v);
    assert!(n.starts_with(&format!("Your books were copied to {}.", path.display())), "{n}");
    assert!(n.contains("USB drive"), "{n}");

    let r = ok_json(&call(&client, "verify_ledger", json!({ "path": path.to_str().unwrap() })).await);
    assert_eq!(r["ok"], true);
    assert!(note(&r).starts_with("The books are intact: 1 entries in "), "{}", note(&r));

    let own = ok_json(&call(&client, "verify_ledger", json!({})).await);
    assert_eq!(own["ok"], true);

    let l = ok_json(&call(&client, "list_backups", json!({})).await);
    assert_eq!(l["backups"].as_array().unwrap().len(), 1);
    assert_eq!(l["folder"], dir.path().join("backups").to_str().unwrap());
    client.cancel().await.ok();
}

#[tokio::test]
async fn admin_list_backups_empty_names_backup_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let v = ok_json(&call(&client, "list_backups", json!({})).await);
    assert_eq!(v["backups"], json!([]));
    assert!(note(&v).contains("backup_ledger"), "{}", note(&v));
    client.cancel().await.ok();
}

#[tokio::test]
async fn admin_restore_goes_back_and_later_calls_read_restored_data() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    expense(&client, "Early one").await;
    expense(&client, "Early two").await;
    let b = ok_json(&call(&client, "backup_ledger", json!({})).await);
    let backup = b["path"].as_str().unwrap().to_string();
    expense(&client, "Late one").await;
    expense(&client, "Late two").await;
    assert_eq!(listed_descriptions(&client).await.len(), 4);

    let v = ok_json(&call(&client, "restore_ledger", json!({ "path": backup, "reason": "went back to the morning backup" })).await);
    assert_eq!(v["restored_from"], backup);
    assert_eq!(v["transaction_count"], 2);
    assert!(v["schema_version"].is_number());
    let safety = std::path::PathBuf::from(v["safety_backup"]["path"].as_str().unwrap());
    assert!(safety.is_file());
    let n = note(&v);
    assert!(n.contains("Your books are now as they were in") && n.contains(safety.to_str().unwrap()), "{n}");

    // Same long-lived server connection reads the restored data.
    let mut got = listed_descriptions(&client).await;
    got.sort();
    assert_eq!(got, vec!["Early one", "Early two"]);
    let info = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(info["transaction_count"], 2);
    // And writes keep working afterwards.
    expense(&client, "After restore").await;
    assert_eq!(listed_descriptions(&client).await.len(), 3);

    // The restore can be undone from the safety copy.
    ok_json(&call(&client, "restore_ledger", json!({ "path": safety.to_str().unwrap(), "reason": "undo" })).await);
    assert_eq!(listed_descriptions(&client).await.len(), 4);
    client.cancel().await.ok();
}

#[tokio::test]
async fn admin_restore_refusals_change_nothing_and_server_keeps_serving() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    expense(&client, "Kept").await;
    let b = ok_json(&call(&client, "backup_ledger", json!({})).await);
    let backup = b["path"].as_str().unwrap().to_string();

    let r = call(&client, "restore_ledger", json!({ "path": backup })).await;
    assert!(is_error(&r));

    let r = call(&client, "restore_ledger", json!({ "path": backup, "reason": "  " })).await;
    assert!(is_error(&r));
    assert!(text(&r).ends_with("Nothing was changed."), "{}", text(&r));

    let not_ledger = dir.path().join("notes.txt");
    std::fs::write(&not_ledger, "just some words, not a database at all, long enough to look like a file").unwrap();
    let r = call(&client, "restore_ledger", json!({ "path": not_ledger.to_str().unwrap(), "reason": "try" })).await;
    assert!(is_error(&r));
    assert!(text(&r).ends_with("Nothing was changed."), "{}", text(&r));

    let r = call(&client, "restore_ledger", json!({ "path": dir.path().join("missing.sqlite3").to_str().unwrap(), "reason": "try" })).await;
    assert!(is_error(&r));
    assert!(text(&r).ends_with("Nothing was changed."), "{}", text(&r));

    assert_eq!(listed_descriptions(&client).await, vec!["Kept"]);
    client.cancel().await.ok();
}

#[tokio::test]
async fn admin_verify_of_a_non_ledger_is_a_plain_error() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let junk = dir.path().join("junk.sqlite3");
    std::fs::write(&junk, "not a database, just text that is long enough to be a file header or so").unwrap();
    let r = call(&client, "verify_ledger", json!({ "path": junk.to_str().unwrap() })).await;
    assert!(is_error(&r));
    assert!(text(&r).contains("Nothing was changed."), "{}", text(&r));
    client.cancel().await.ok();
}

#[tokio::test]
async fn admin_full_number_guard_still_applies_to_path_and_folder() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    for (tool, args) in [
        ("backup_ledger", json!({ "folder": "/tmp/4111111111111111" })),
        ("list_backups", json!({ "folder": "/tmp/4111111111111111" })),
        ("verify_ledger", json!({ "path": "/tmp/4111111111111111.sqlite3" })),
        ("restore_ledger", json!({ "path": "/tmp/4111111111111111.sqlite3", "reason": "x" })),
    ] {
        let r = call(&client, tool, args).await;
        assert!(is_error(&r), "{tool}");
        assert!(!text(&r).contains("4111111111111111"), "{tool}: {}", text(&r));
    }
    assert!(!dir.path().join("backups").exists());
    client.cancel().await.ok();
}

#[tokio::test]
async fn admin_ledger_info_reports_app_version_and_supported_schema() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;
    let v = ok_json(&call(&client, "ledger_info", json!({})).await);
    assert_eq!(v["app_version"], ledger_core::app_version());
    assert_eq!(v["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["supported_schema"], 4);
    client.cancel().await.ok();
}
