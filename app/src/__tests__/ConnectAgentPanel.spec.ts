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
  const claude_code_command = `claude mcp add freehold-ledger ${overridden ? `-e FREEHOLD_LEDGER_DB="${LEDGER}" ` : ""}-- "${server}"`;
  const claude_desktop_json = `{\n  "mcpServers": {\n    "freehold-ledger": {\n      "command": "${server}"${env}\n    }\n  }\n}`;
  const envLine = overridden ? `\n- environment: FREEHOLD_LEDGER_DB=${LEDGER}` : "";
  const agent_message = `Please connect yourself to Freehold Ledger, my bookkeeping app, so you can keep my books for me.\n\n- command: ${server}${envLine}\n\nIf you are Claude Code, run this command:\n${claude_code_command}\n\nmerge this into it:\n${claude_desktop_json}\n\ncall the ledger_info tool`;
  return {
    agent_message,
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

  it("s_con_1_shows_one_message_and_no_per_client_sections", async () => {
    const wrapper = await openPanel();
    const dialog = wrapper.get("dialog");
    expect(dialog.text()).toContain("Connect your agent");
    expect(dialog.text()).toContain("Copy this message and send it to your AI agent. It will set itself up.");
    for (const h of ["Claude Desktop", "Other agents"]) expect(dialog.text()).not.toContain(h);
    expect(dialog.findAll("h3")).toHaveLength(0);
    const blocks = dialog.findAll("pre");
    expect(blocks).toHaveLength(1);
    expect(blocks[0].text()).toBe(info().agent_message);
    expect(blocks[0].text()).toContain(BUNDLED);
    expect(dialog.text()).toContain(
      "Works with AI apps on this computer that support MCP. Browser and phone chats can't connect to a local app yet.",
    );
    expect(dialog.findAll("button").filter((b) => b.text() === "Copy")).toHaveLength(1);
    expect(dialog.text()).not.toContain("development build");
    wrapper.unmount();
  });

  it("s_con_2_dev_build_shows_dev_caption", async () => {
    const wrapper = await openPanel(info({ server_path: DEV }));
    const dialog = wrapper.get("dialog");
    expect(dialog.text()).toContain(
      "You're running a development build. This path changes once the app is installed in Applications.",
    );
    expect(dialog.get("pre").text()).toContain(DEV);
    wrapper.unmount();
  });

  it("override_shows_env_in_the_message_only_when_overridden", async () => {
    const wrapper = await openPanel(info({ ledger_path_overridden: true }));
    const msg = wrapper.get("dialog").get("pre").text();
    expect(msg).toContain(`FREEHOLD_LEDGER_DB=${LEDGER}`);
    expect(msg).toContain(`-e FREEHOLD_LEDGER_DB="${LEDGER}" --`);
    wrapper.unmount();

    const plain = await openPanel();
    expect(plain.get("dialog").get("pre").text()).not.toContain("FREEHOLD_LEDGER_DB");
    plain.unmount();
  });

  it("copy_writes_the_message_shows_copied_and_announces", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const wrapper = await openPanel();
    const dialog = wrapper.get("dialog");
    const copy = dialog.findAll("button").find((b) => b.text() === "Copy")!;
    await copy.trigger("click");
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith(info().agent_message);
    expect(dialog.findAll("button").some((b) => b.text() === "Copied")).toBe(true);
    expect(dialog.get('[aria-live="polite"]').text()).toContain("Copied");
    vi.advanceTimersByTime(2100);
    await flushPromises();
    expect(dialog.findAll("button").some((b) => b.text() === "Copied")).toBe(false);
    vi.useRealTimers();
    wrapper.unmount();
  });

  it("copy_failure_says_so_and_does_not_show_copied", async () => {
    writeText.mockRejectedValue(new Error("denied"));
    const wrapper = await openPanel();
    const dialog = wrapper.get("dialog");
    await dialog.findAll("button").find((b) => b.text() === "Copy")!.trigger("click");
    await flushPromises();
    expect(dialog.get('[aria-live="polite"]').text()).toBe(
      "Could not copy. Select the text and copy it by hand.",
    );
    expect(dialog.findAll("button").some((b) => b.text() === "Copied")).toBe(false);
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
    expect(wrapper.get("dialog").text()).toContain("Copy this message and send it to your AI agent");
    wrapper.unmount();
  });
});
