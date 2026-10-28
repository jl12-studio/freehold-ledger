import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

const invoke = vi.fn();
const writeText = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: (...a: unknown[]) => writeText(...a),
}));

import App from "../App.vue";

const LEDGER = "/Users/pat/Library/Application Support/org.freeholders.ledger/ledger.sqlite3";
const BUNDLED = "/Applications/Freehold Ledger.app/Contents/MacOS/ledger-mcp";
const DEV = "/repo/target/debug/ledger-mcp";

function info(over: Record<string, unknown> = {}) {
  const server = (over.server_path as string) ?? BUNDLED;
  const overridden = Boolean(over.ledger_path_overridden);
  const env = overridden ? `,\n      "env": {\n        "FREEHOLD_LEDGER_DB": "${LEDGER}"\n      }` : "";
  return {
    server_path: server,
    bundled: server.includes(".app/Contents/MacOS"),
    ledger_path: LEDGER,
    ledger_path_overridden: overridden,
    claude_desktop_json: `{\n  "mcpServers": {\n    "freehold-ledger": {\n      "command": "${server}"${env}\n    }\n  }\n}`,
    claude_code_command: `claude mcp add freehold-ledger ${overridden ? `-e FREEHOLD_LEDGER_DB="${LEDGER}" ` : ""}-- "${server}"`,
    generic: `Any MCP client that can launch a stdio server: command ${server}, no arguments.`,
  };
}

function route(connection: ReturnType<typeof info>) {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "ledger_info") return { path: LEDGER, schema_version: 1, transaction_count: 0 };
    if (cmd === "mcp_connection_info") return connection;
    if (cmd === "ledger_years") return [2026];
    throw new Error(`unexpected ${cmd}`);
  });
}

async function openPanel(connection = info()) {
  route(connection);
  const wrapper = mount(App, { attachTo: document.body });
  await flushPromises();
  const opener = wrapper.findAll("button").find((b) => b.text() === "Connect your agent")!;
  await opener.trigger("click");
  await flushPromises();
  return wrapper;
}

describe("Connect your agent panel", () => {
  beforeEach(() => {
    invoke.mockReset();
    writeText.mockReset();
    writeText.mockResolvedValue(undefined);
    document.body.innerHTML = "";
  });

  it("s_con_1_shows_bundled_path_and_three_snippets", async () => {
    const wrapper = await openPanel();
    const dialog = wrapper.get("dialog");
    expect(dialog.text()).toContain("Connect your agent");
    for (const h of ["Claude Desktop", "Claude Code", "Other agents"]) expect(dialog.text()).toContain(h);
    const blocks = dialog.findAll("pre").map((p) => p.text());
    expect(blocks).toHaveLength(3);
    for (const b of blocks) expect(b).toContain(BUNDLED);
    expect(blocks[1]).toBe(`claude mcp add freehold-ledger -- "${BUNDLED}"`);
    expect(dialog.text()).not.toContain("development build");
    expect(dialog.text()).toContain("What's in my ledger?");
    wrapper.unmount();
  });

  it("s_con_2_dev_build_shows_dev_caption", async () => {
    const wrapper = await openPanel(info({ server_path: DEV }));
    const dialog = wrapper.get("dialog");
    expect(dialog.text()).toContain(
      "You're running a development build. This path changes once the app is installed in Applications.",
    );
    expect(dialog.findAll("pre")[0].text()).toContain(DEV);
    wrapper.unmount();
  });

  it("override_adds_env_to_both_snippets", async () => {
    const wrapper = await openPanel(info({ ledger_path_overridden: true }));
    const blocks = wrapper.get("dialog").findAll("pre").map((p) => p.text());
    expect(blocks[0]).toContain(`"FREEHOLD_LEDGER_DB": "${LEDGER}"`);
    expect(blocks[1]).toContain(`-e FREEHOLD_LEDGER_DB="${LEDGER}" --`);
    wrapper.unmount();

    const plain = await openPanel();
    const p = plain.get("dialog").findAll("pre").map((x) => x.text());
    expect(p[0]).not.toContain("FREEHOLD_LEDGER_DB");
    expect(p[1]).not.toContain("FREEHOLD_LEDGER_DB");
    plain.unmount();
  });

  it("copy_shows_copied_and_announces", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const wrapper = await openPanel();
    const dialog = wrapper.get("dialog");
    const copy = dialog.findAll("button").filter((b) => b.text() === "Copy")[1];
    await copy.trigger("click");
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith(`claude mcp add freehold-ledger -- "${BUNDLED}"`);
    expect(dialog.findAll("button").some((b) => b.text() === "Copied")).toBe(true);
    expect(dialog.get('[aria-live="polite"]').text()).toContain("Copied");
    vi.advanceTimersByTime(2100);
    await flushPromises();
    expect(dialog.findAll("button").some((b) => b.text() === "Copied")).toBe(false);
    vi.useRealTimers();
    wrapper.unmount();
  });

  it("empty_state_primary_action_opens_panel", async () => {
    route(info());
    const wrapper = mount(App, { attachTo: document.body });
    await flushPromises();
    expect(wrapper.text()).toContain(
      "Nothing has been recorded yet. Connect your agent and tell it what you bought or earned.",
    );
    const main = wrapper.get("main");
    const primary = main.findAll("button")[0];
    expect(primary.text()).toBe("Connect your agent");
    expect(main.text()).toContain("Show the ledger file");
    await primary.trigger("click");
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith("mcp_connection_info");
    expect(wrapper.get("dialog").text()).toContain("Claude Desktop");
    wrapper.unmount();
  });
});
