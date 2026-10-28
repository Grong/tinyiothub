import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiClient } from "../api/client.js";
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

describe("app router · Runtime hash routes (default-off Workbench)", () => {
  afterEach(() => {
    window.history.pushState({}, "", "/");
  });

  it("does not register #/runtime while runtime_workbench_enabled is false", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    const el = makeApp(true);
    el.handleRoute();
    expect(el.runtimeWorkbenchEnabled).toBe(false);
    expect(el.currentRoute).toBe("dashboard");
    expect(el.runtimeRoute).toBeNull();
  });

  it("routes list/new/detail explicitly only when the Workbench flag is true", () => {
    const el = makeApp(true);
    el.runtimeWorkbenchEnabled = true;

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

  it("still requires authentication when Runtime is enabled", () => {
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    const el = makeApp(false);
    el.runtimeWorkbenchEnabled = true;
    el.handleRoute();
    expect(el.currentRoute).toBe("login");
  });
});

describe("app Runtime production composition", () => {
  let app: any;

  afterEach(() => {
    app?.remove();
    app = undefined;
    localStorage.clear();
    sessionStorage.clear();
    vi.restoreAllMocks();
    window.history.pushState({}, "", "/");
  });

  function mockWorkspaceConfig(config: unknown) {
    vi.spyOn(ApiClient, "get").mockImplementation(async (endpoint: string) => ({
      code: 0,
      msg: "",
      result: endpoint.endsWith("/heartbeat/config")
        ? config
        : endpoint === "/agents"
          ? { agents: [{ id: "agent-1", workspaceId: "ws-enabled" }] }
          : null,
    }) as never);
  }

  it("loads the selected workspace flag to enable Runtime routes and the AI-center link", async () => {
    mockWorkspaceConfig({ runtimeWorkbenchEnabled: true });
    sessionStorage.setItem("auth-token", "token");
    localStorage.setItem("workspace-id", "ws-enabled");
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    app = document.createElement("tinyiothub-app");
    document.body.append(app);

    await vi.waitFor(() => expect(app.currentRoute).toBe("runtime"));
    expect(ApiClient.get).toHaveBeenCalledWith("/workspaces/ws-enabled/heartbeat/config");
    app.currentRoute = "ai-ops";
    await app.updateComplete;
    const aiOps = app.querySelector("view-ai-ops");
    aiOps.wsLoading = false;
    aiOps.selectedWsId = "ws-enabled";
    await aiOps.updateComplete;
    expect(aiOps.runtimeWorkbenchEnabled).toBe(true);
    await vi.waitFor(() => expect(aiOps.querySelector('a[href="#/runtime"]')).not.toBeNull());
  });

  it("fails closed for an absent flag and reloads when the selected workspace changes", async () => {
    mockWorkspaceConfig(null);
    sessionStorage.setItem("auth-token", "token");
    localStorage.setItem("workspace-id", "ws-default");
    window.history.pushState({}, "", "/dashboard");
    window.location.hash = "#/runtime";
    app = document.createElement("tinyiothub-app");
    document.body.append(app);

    await vi.waitFor(() => expect(ApiClient.get).toHaveBeenCalledWith("/workspaces/ws-default/heartbeat/config"));
    expect(app.runtimeWorkbenchEnabled).toBe(false);
    expect(app.currentRoute).toBe("dashboard");
    expect(app.runtimeRoute).toBeNull();

    vi.mocked(ApiClient.get).mockImplementation(async (endpoint: string) => ({
      code: 0,
      msg: "",
      result: endpoint === "/workspaces/ws-enabled/heartbeat/config"
        ? { runtimeWorkbenchEnabled: true }
        : null,
    }) as never);
    localStorage.setItem("workspace-id", "ws-enabled");
    window.dispatchEvent(new CustomEvent("workspace-changed"));
    await vi.waitFor(() => expect(app.currentRoute).toBe("runtime"));
    expect(app.runtimeWorkbenchEnabled).toBe(true);
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
