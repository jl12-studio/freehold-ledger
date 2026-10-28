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

const TEN_TOOLS: [&str; 10] = [
    "correct_transaction",
    "export_tax_year",
    "ledger_info",
    "list_categories",
    "list_transactions",
    "reclassify_transaction",
    "record_expense",
    "record_income",
    "summarize",
    "void_transaction",
];

#[tokio::test]
async fn s_mcp_1_lists_ten_tools_with_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    let client = spawn_server(&dir.path().join("ledger.sqlite3")).await;

    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(names, TEN_TOOLS);

    for t in &tools {
        let desc = t.description.as_deref().unwrap_or("");
        assert!(!desc.trim().is_empty(), "{} has no description", t.name);
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
    assert!(voided["note"].as_str().unwrap().starts_with("Voided, not deleted"));
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
    assert_eq!(info["schema_version"], 1);
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
    assert_eq!(folder, out.path().join("Freehold Ledger 2026 export"));
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
