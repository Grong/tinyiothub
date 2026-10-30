import { render } from "lit";
import { afterEach, describe, expect, it } from "vitest";
import "./app.js";
import "./views/ai-ops.js";
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

describe("app navigation · Runtime entry", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  function renderNav(el: any) {
    const host = document.createElement("div");
    render(el.renderNav(), host);
    return host;
  }

  it("shows the Runtime nav item and opens the hash route", () => {
    const el = makeApp(true);
    const link = renderNav(el).querySelector(
      'a[href="#/runtime"]',
    ) as HTMLAnchorElement;
    expect(link).not.toBeNull();
    link.click();
    expect(window.location.hash).toBe("#/runtime");
    expect(el.currentRoute).toBe("runtime");
    expect(el.runtimeRoute).toMatchObject({ name: "list" });
  });

  it("marks the Runtime entry active on list, new, and task routes", () => {
    const el = makeApp(true);
    for (const route of ["runtime", "runtime-new", "runtime-task"]) {
      el.currentRoute = route;
      const link = renderNav(el).querySelector('a[href="#/runtime"]');
      expect(link?.classList.contains("active")).toBe(true);
    }
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
