import { render } from "lit";
import { afterEach, describe, expect, it } from "vitest";
import "./app.js";
import "./views/chat.js";

function makeApp(authenticated: boolean) {
  // Not connected to the DOM: connectedCallback (auth check, user info fetch)
  // never runs; handleRoute only touches history/state.
  const el = document.createElement("tinyiothub-app") as any;
  el.isAuthenticated = authenticated;
  return el;
}

describe("app router root path", () => {
  it("resolves / to home for authenticated users (no chat redirect)", () => {
    window.history.pushState({}, "", "/");
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("home");
    expect(window.location.pathname).toBe("/");
  });

  it("resolves / to home for anonymous users", () => {
    window.history.pushState({}, "", "/");
    const el = makeApp(false);
    el.handleRoute();
    expect(el.currentRoute).toBe("home");
  });

  it("still guards protected routes for anonymous users", () => {
    window.history.pushState({}, "", "/dashboard");
    const el = makeApp(false);
    el.handleRoute();
    expect(el.currentRoute).toBe("login");
  });
});

describe("app router · assistant hash routes", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("maps #/assistant onto the existing chat view", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/assistant";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("chat");
    expect(el.assistantRoute).toEqual({ view: "assistant" });
    expect(window.location.pathname).toBe("/dashboard");
  });

  it("maps a session deep link and its focus state onto the chat view", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash =
      "#/assistant/sessions/agent%3Aws%3Adefault%2Fs1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("chat");
    expect(el.assistantRoute).toEqual({
      view: "assistant",
      sessionKey: "agent:ws:default/s1",
      focus: {
        view: "flow",
        device: "dev/1",
        at: "2026-10-27T10:00:00Z",
        event: "event:2",
      },
    });
  });

  it("still requires authentication", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/assistant";
    const el = makeApp(false);
    el.handleRoute();
    expect(el.currentRoute).toBe("login");
  });
});

describe("app router · legacy workbench redirects", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("redirects every legacy Runtime and Harness deep link to #/assistant", () => {
    for (const hash of [
      "#/runtime",
      "#/runtime/new",
      "#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&event=event%3A2",
      "#/harness",
      "#/harness#main-main%3A1",
      "#/harness/tasks",
      "#/harness/tasks/new",
      "#/harness/tasks/task%3A1?from=%23%2Fharness%23main-main%3A1",
      "#/harness/workflows/wf%3A1",
      "#/harness/supervisor",
    ]) {
      window.history.pushState({}, "", "/dashboard");
      window.location.hash = hash;
      const el = makeApp(true);
      el.handleRoute();
      expect(window.location.hash, hash).toBe("#/assistant");
      expect(el.currentRoute, hash).toBe("chat");
      // The legacy Task id is deliberately not reused as a session key.
      expect(el.assistantRoute, hash).toEqual({ view: "assistant" });
      expect(window.location.pathname, hash).toBe("/dashboard");
    }
  });

  it("still requires authentication", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    const el = makeApp(false);
    el.handleRoute();
    expect(el.currentRoute).toBe("login");
  });

  it("leaves unknown legacy hashes on the pathname router", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/harness/unknown";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("dashboard");
    expect(window.location.hash).toBe("#/harness/unknown");
  });
});

describe("app navigation · assistant entry", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  function renderNav(el: any) {
    const host = document.createElement("div");
    render(el.renderNav(), host);
    return host;
  }

  it("keeps the assistant item and removes the Harness/Runtime entries", () => {
    const el = makeApp(true);
    const nav = renderNav(el);
    const assistant = nav.querySelector('a[href="/chat"]') as HTMLAnchorElement;
    expect(assistant).not.toBeNull();
    expect(assistant.textContent).toContain("AI 助手");
    expect(nav.querySelector('a[href="#/harness"]')).toBeNull();
    expect(nav.querySelector('a[href="#/runtime"]')).toBeNull();
  });

  it("marks the assistant item active on the chat route", () => {
    const el = makeApp(true);
    el.currentRoute = "chat";
    const link = renderNav(el).querySelector('a[href="/chat"]');
    expect(link?.classList.contains("active")).toBe(true);
  });
});

describe("app router · run 详情页 hash 路由（T13）", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("#/ai-ops/runs/{id} 路由到 run-detail，id 容忍冒号（patrol:p1，T12 M-1 结转）", () => {
    window.history.pushState({}, "", "/dispositions");
    window.location.hash = "#/ai-ops/runs/patrol:p1";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("run-detail");
    expect(el.runDetailId).toBe("patrol:p1");
  });

  it("普通 runId（run-9）同样命中", () => {
    window.history.pushState({}, "", "/dispositions");
    window.location.hash = "#/ai-ops/runs/run-9";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("run-detail");
    expect(el.runDetailId).toBe("run-9");
  });

  it("未登录访问 run 详情 → login", () => {
    window.history.pushState({}, "", "/dispositions");
    window.location.hash = "#/ai-ops/runs/run-9";
    const el = makeApp(false);
    el.handleRoute();
    expect(el.currentRoute).toBe("login");
  });

  it("非匹配 hash 不影响 pathname 路由", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/ai-ops";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("dashboard");
  });
});
