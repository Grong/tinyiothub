import { afterEach, describe, expect, it } from "vitest";
import "./app.js";

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
