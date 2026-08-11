import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { bootstrap, isWindowMaximized, listenLogs } from "./tauri";

vi.mock("./tauri", () => ({
  bootstrap: vi.fn(),
  cancelLatency: vi.fn(),
  createSubscription: vi.fn(),
  deleteSelectedNodes: vi.fn(),
  deleteSubscription: vi.fn(),
  getNodeTrafficPanel: vi.fn(),
  hideWindowToTray: vi.fn(),
  importSubscription: vi.fn(),
  isWindowMaximized: vi.fn(),
  listenLatencyResults: vi.fn(),
  listenLogs: vi.fn(),
  minimizeWindow: vi.fn(),
  reorderSubscriptions: vi.fn(),
  saveNodeRemark: vi.fn(),
  saveProxySettings: vi.fn(),
  saveSelection: vi.fn(),
  startMihomo: vi.fn(),
  startWindowDragging: vi.fn(),
  stopMihomo: vi.fn(),
  testLatency: vi.fn(),
  toggleMaximizeWindow: vi.fn(),
  updateSubscription: vi.fn(),
}));

type RenderResult = {
  container: HTMLDivElement;
  root: Root;
};

beforeAll(() => {
  Object.assign(globalThis, {
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: vi.fn(),
  });
});

afterEach(() => {
  document.body.innerHTML = "";
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  vi.clearAllMocks();
  vi.useRealTimers();
});

async function renderApp(): Promise<RenderResult> {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);

  await act(async () => {
    root.render(<App />);
  });

  return { container, root };
}

function findButton(container: HTMLElement, text: string) {
  const button = Array.from(container.querySelectorAll("button")).find((item) =>
    item.textContent?.includes(text),
  );
  if (!button) {
    throw new Error(`Button not found: ${text}`);
  }
  return button as HTMLButtonElement;
}

describe("App shell", () => {
  it("shows a retryable error when desktop bootstrap fails", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    vi.mocked(bootstrap).mockRejectedValueOnce(new Error("配置文件损坏"));
    vi.mocked(isWindowMaximized).mockResolvedValue(false);
    vi.mocked(listenLogs).mockResolvedValue(() => undefined);

    const { container, root } = await renderApp();

    expect(container.textContent).toContain("应用启动失败");
    expect(container.textContent).toContain("配置文件损坏");
    expect(findButton(container, "重新加载")).toBeDefined();

    await act(async () => {
      root.unmount();
    });
  });

  it("navigates between the primary workspaces", async () => {
    const { container, root } = await renderApp();

    expect(container.textContent).toContain("每个端口是一个稳定的本地代理");

    await act(async () => {
      findButton(container, "节点库").click();
    });
    expect(container.textContent).toContain("浏览所有订阅节点");
    expect(container.textContent).toContain("美国 - 洛杉矶 02");

    await act(async () => {
      findButton(container, "订阅管理").click();
    });
    expect(container.textContent).toContain("管理订阅来源");
    expect(container.textContent).toContain("个人订阅");

    await act(async () => {
      root.unmount();
    });
  });

  it("consumes the add-port request only once", async () => {
    const { container, root } = await renderApp();

    await act(async () => {
      findButton(container, "新增端口").click();
    });
    expect(container.textContent).toContain("创建端口");

    await act(async () => {
      findButton(container, "取消").click();
      findButton(container, "节点库").click();
      findButton(container, "端口管理").click();
    });
    expect(container.textContent).not.toContain("创建端口");

    await act(async () => root.unmount());
  });

  it("renders the current full-width port layout with aligned table columns", async () => {
    const { container, root } = await renderApp();

    expect(container.textContent).toContain("流量概览");
    expect(container.textContent).not.toContain("暂无活动记录");

    const firstRow = container.querySelector<HTMLTableRowElement>("tbody tr");
    const cells = firstRow ? Array.from(firstRow.cells) : [];
    expect(cells).toHaveLength(6);
    expect(cells[0].className).not.toBe(cells[5].className);
    expect(cells[5].className).not.toBe("");
    const tableWrap = container.querySelector("table")?.parentElement;
    const tableCard = tableWrap?.parentElement;
    const main = tableCard?.parentElement;
    const layout = main?.parentElement;
    expect(getComputedStyle(layout as Element).display).toBe("flex");
    expect(getComputedStyle(layout as Element).flexDirection).toBe("column");
    expect(getComputedStyle(main as Element).minHeight).toBe("0px");
    expect(getComputedStyle(tableWrap as Element).overflow).toBe("auto");

    await act(async () => root.unmount());
  });

  it("deletes a port immediately from the action icon without confirmation", async () => {
    const { container, root } = await renderApp();
    const deleteButton = container.querySelector<HTMLButtonElement>(
      'button[aria-label^="删除端口 "]',
    );
    expect(deleteButton).not.toBeNull();
    const row = deleteButton?.closest("tr");
    const portName = deleteButton?.getAttribute("aria-label")?.replace("删除端口 ", "");
    expect(row).not.toBeNull();
    expect(portName).toBeTruthy();

    await act(async () => {
      deleteButton?.click();
    });

    expect(row?.isConnected).toBe(false);
    expect(container.textContent).not.toContain(portName);
    expect(container.querySelector('[role="alertdialog"]')).toBeNull();

    await act(async () => root.unmount());
  });

  it("queues consecutive latency tests instead of failing the second one", async () => {
    const { container, root } = await renderApp();
    await act(async () => {
      findButton(container, "节点库").click();
    });
    vi.useFakeTimers();
    const testButtons = Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
      .filter((button) => button.textContent === "测速");
    expect(testButtons.length).toBeGreaterThan(1);

    await act(async () => {
      testButtons[0].click();
      testButtons[1].click();
    });
    expect(container.textContent).toContain("排队中...");
    expect(container.textContent).not.toContain("失败");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    expect(container.textContent).not.toContain("排队中...");

    await act(async () => root.unmount());
  });

  it("updates the visible core state in browser preview", async () => {
    const { container, root } = await renderApp();

    expect(container.textContent).toContain("核心运行中");
    expect(container.textContent).toContain("停止核心");

    await act(async () => {
      findButton(container, "停止核心").click();
    });
    expect(container.textContent).toContain("核心待命");
    expect(container.textContent).toContain("启动核心");

    await act(async () => {
      root.unmount();
    });
  });

  it("blocks saving an invalid enabled local proxy", async () => {
    const { container, root } = await renderApp();

    await act(async () => {
      findButton(container, "设置").click();
    });

    const proxySwitch = container.querySelector<HTMLButtonElement>(
      'button[aria-label="启用本地代理"]',
    );
    const proxyInput = container.querySelector<HTMLInputElement>(
      'input[placeholder="例如: http://127.0.0.1:20122"]',
    );
    expect(proxySwitch).not.toBeNull();
    expect(proxyInput).not.toBeNull();

    await act(async () => {
      proxySwitch?.click();
      const valueSetter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      valueSetter?.call(proxyInput, "not-a-url");
      proxyInput?.dispatchEvent(new Event("input", { bubbles: true }));
    });

    expect(container.textContent).toContain("请输入完整的 http:// 或 https:// 代理地址");
    expect(proxyInput?.getAttribute("aria-invalid")).toBe("true");
    expect(findButton(container, "保存设置").disabled).toBe(true);

    await act(async () => {
      root.unmount();
    });
  });

  it("supports keyboard dismissal for the port action menu", async () => {
    const { container, root } = await renderApp();
    const menuButton = container.querySelector<HTMLButtonElement>('button[aria-label="更多操作"]');
    expect(menuButton).not.toBeNull();

    await act(async () => {
      menuButton?.click();
    });
    const menu = container.querySelector<HTMLElement>('[role="menu"]');
    expect(menu).not.toBeNull();
    expect(menuButton?.getAttribute("aria-expanded")).toBe("true");
    expect(document.activeElement?.getAttribute("role")).toBe("menuitem");

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
      await new Promise((resolve) => window.setTimeout(resolve, 0));
    });
    expect(container.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(menuButton);

    await act(async () => root.unmount());
  });

  it("supports keyboard navigation across activity tabs", async () => {
    const { container, root } = await renderApp();
    await act(async () => {
      findButton(container, "连接活动").click();
    });

    const connectionsTab = container.querySelector<HTMLButtonElement>("#activity-tab-connections");
    const logsTab = container.querySelector<HTMLButtonElement>("#activity-tab-logs");
    expect(connectionsTab?.tabIndex).toBe(0);
    expect(logsTab?.tabIndex).toBe(-1);

    connectionsTab?.focus();
    await act(async () => {
      connectionsTab?.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    });
    expect(document.activeElement).toBe(logsTab);
    expect(logsTab?.getAttribute("aria-selected")).toBe("true");
    expect(container.querySelector('[role="tabpanel"]')?.id).toBe("activity-panel-logs");

    await act(async () => root.unmount());
  });
});
