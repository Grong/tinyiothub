import { render } from "lit";
import { afterEach, describe, expect, it } from "vitest";
import "./app.js";
import "./views/ai-ops.js";
import "./views/harness-overview.js";
import "./views/runtime-list.js";
import "./views/runtime-new.js";
import "./views/runtime-task.js";

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

describe("app router · Runtime hash routes", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("routes list/new/detail explicitly", () => {
    const el = makeApp(true);

    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    el.handleRoute();
    expect(el.currentRoute).toBe("runtime");

    window.location.hash = "#/runtime/new";
    el.handleRoute();
    expect(el.currentRoute).toBe("runtime-new");

    window.location.hash = "#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&event=event_2";
    el.handleRoute();
    expect(el.currentRoute).toBe("runtime-task");
    expect(el.runtimeRoute).toMatchObject({
      name: "task", taskId: "task:1", view: "flow", device: "dev/1", history: { event: "event_2" },
    });
  });

  it("still requires authentication", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    const el = makeApp(false);
    el.handleRoute();
    expect(el.currentRoute).toBe("login");
  });
});

describe("app navigation · Harness entry", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  function renderNav(el: any) {
    const host = document.createElement("div");
    render(el.renderNav(), host);
    return host;
  }

  it("shows the Harness nav item and opens the canonical hash route", () => {
    const el = makeApp(true);
    const link = renderNav(el).querySelector(
      'a[href="#/harness"]',
    ) as HTMLAnchorElement;
    expect(link).not.toBeNull();
    expect(link.textContent).toContain("Harness");
    link.click();
    expect(window.location.hash).toBe("#/harness");
    expect(el.currentRoute).toBe("harness");
    expect(el.harnessRoute).toEqual({ name: "overview" });
    expect(renderNav(el).querySelector('a[href="#/runtime"]')).toBeNull();
  });

  it("marks the Harness entry active on overview, Task list, new, and Task routes", () => {
    const el = makeApp(true);
    for (const route of ["harness", "runtime", "runtime-new", "runtime-task"]) {
      el.currentRoute = route;
      const link = renderNav(el).querySelector('a[href="#/harness"]');
      expect(link?.classList.contains("active")).toBe(true);
    }
  });
});

describe("app router · canonical Harness hash routes", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("routes the overview, Task list, new Task and Task detail explicitly", () => {
    window.history.pushState({}, "", "/dashboard");
    const el = makeApp(true);

    window.location.hash = "#/harness";
    el.handleRoute();
    expect(el.currentRoute).toBe("harness");
    expect(el.harnessRoute).toEqual({ name: "overview" });

    window.location.hash = "#/harness/workflows/wf%3A1";
    el.handleRoute();
    expect(el.currentRoute).toBe("harness");
    expect(el.harnessRoute).toEqual({
      name: "workflows",
      workflowId: "wf:1",
      creating: false,
    });

    window.location.hash = "#/harness/tasks";
    el.handleRoute();
    expect(el.currentRoute).toBe("runtime");
    expect(el.runtimeRoute).toEqual({ name: "list" });

    window.location.hash = "#/harness/tasks/new";
    el.handleRoute();
    expect(el.currentRoute).toBe("runtime-new");
    expect(el.runtimeRoute).toEqual({ name: "new" });

    window.location.hash =
      "#/harness/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2";
    el.handleRoute();
    expect(el.currentRoute).toBe("runtime-task");
    expect(el.runtimeRoute).toMatchObject({
      name: "task",
      taskId: "task:1",
      view: "flow",
      device: "dev/1",
      history: { at: "2026-10-27T10:00:00Z", event: "event:2" },
    });
  });

  it("redirects legacy Runtime links to canonical Harness URLs without losing state", () => {
    window.history.pushState({}, "", "/dashboard");
    const el = makeApp(true);

    window.location.hash = "#/runtime";
    el.handleRoute();
    expect(window.location.hash).toBe("#/harness/tasks");
    expect(el.currentRoute).toBe("runtime");

    window.location.hash = "#/runtime/new";
    el.handleRoute();
    expect(window.location.hash).toBe("#/harness/tasks/new");
    expect(el.currentRoute).toBe("runtime-new");

    window.location.hash =
      "#/runtime/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2";
    el.handleRoute();
    expect(window.location.hash).toBe(
      "#/harness/tasks/task%3A1?view=flow&device=dev%2F1&at=2026-10-27T10%3A00%3A00Z&event=event%3A2",
    );
    expect(el.currentRoute).toBe("runtime-task");
    expect(el.runtimeRoute).toMatchObject({
      taskId: "task:1",
      view: "flow",
      device: "dev/1",
      history: { at: "2026-10-27T10:00:00Z", event: "event:2" },
    });
  });

  it("uses the shared Runtime Task component and restores the parent return path", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash =
      "#/harness/tasks/child%3A1?from=%23%2Fharness%23main-main%3A1";
    const el = makeApp(true);
    el.handleRoute();

    const host = document.createElement("div");
    render(el.renderPage(), host);
    expect(host.querySelector("view-runtime-task")).not.toBeNull();
    expect(host.querySelector(".harness-task-return a")?.getAttribute("href")).toBe(
      "#/harness#main-main:1",
    );
  });

  it("keeps the parent return path when the shared Task view emits a legacy URL", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash =
      "#/harness/tasks/child%3A1?from=%23%2Fharness%23main-main%3A1";
    const el = makeApp(true);
    el.handleRoute();

    window.location.hash = "#/runtime/tasks/child%3A1?view=flow";
    el.handleRoute();

    expect(window.location.hash).toBe(
      "#/harness/tasks/child%3A1?view=flow&from=%23%2Fharness%23main-main%3A1",
    );
    expect(el.harnessRoute).toMatchObject({
      name: "task",
      taskId: "child:1",
      view: "flow",
      from: "#/harness#main-main:1",
    });
  });

  it("restores the overview from a parent-return anchor link", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/harness#main-main%3A1";
    const el = makeApp(true);
    el.handleRoute();

    expect(el.currentRoute).toBe("harness");
    expect(el.harnessRoute).toEqual({ name: "overview", anchor: "main-main:1" });

    const host = document.createElement("div");
    render(el.renderPage(), host);
    expect((host.querySelector("view-harness-overview") as any)?.anchor).toBe(
      "main-main:1",
    );
  });

  it("falls through invalid Harness routes instead of rendering the overview", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/harness/unknown";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.currentRoute).toBe("dashboard");
    expect(el.harnessRoute).toBeNull();
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
