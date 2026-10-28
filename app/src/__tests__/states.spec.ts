import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import EmptyState from "../components/EmptyState.vue";
import ErrorState from "../components/ErrorState.vue";

describe("states use the shared classes", () => {
  it("s_ui_25_empty_state_buttons", () => {
    const w = mount(EmptyState, { props: { businessName: "Test shop", currentName: "My business", isCurrent: false } });
    const [connect, show] = w.findAll("button");
    expect(connect.text()).toBe("Connect your agent");
    expect(connect.classes()).toEqual(expect.arrayContaining(["btn", "btn-primary"]));
    expect(show.text()).toBe("Show the ledger file");
    expect(show.classes()).toContain("link");
  });

  it("s_ui_25_error_state_button", () => {
    const w = mount(ErrorState, { props: { error: { kind: "io", message: "Could not read it.", path: "/x/l.sqlite3" } } });
    const b = w.get("button");
    expect(b.text()).toBe("Show in Finder");
    expect(b.classes()).toContain("btn");
  });
});
