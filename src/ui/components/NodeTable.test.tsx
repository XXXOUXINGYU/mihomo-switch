import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { SubscriptionRecord } from "../types";
import { NodeTable } from "./NodeTable";

const subscription: SubscriptionRecord = {
  id: "sub-a",
  name: "Sort Feed",
  url: "",
  ua: "mihomo-switch",
  start_port: 10801,
  manual: false,
  content: "",
  nodes: [
    { name: "Charlie 03", type: "vmess", server: "c.example.dev", port: 443 },
    { name: "Alpha 01", type: "vless", server: "a.example.dev", port: 8443 },
    { name: "Bravo 02", type: "trojan", server: "b.example.dev", port: 9443 },
  ],
  selected_node_indices: [0, 2],
  port_assignments: {
    "0": 2080,
    "2": 1080,
  },
  node_remarks: {},
};

type RenderResult = {
  container: HTMLDivElement;
  root: Root;
};

beforeAll(() => {
  Object.assign(globalThis, {
    IS_REACT_ACT_ENVIRONMENT: true,
  });
});

afterEach(() => {
  document.body.innerHTML = "";
});

async function renderNodeTable(
  overrides: Partial<Parameters<typeof NodeTable>[0]> = {},
): Promise<RenderResult> {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);

  await act(async () => {
    root.render(
      <NodeTable
        subscription={subscription}
        latencyMap={{
          0: "300ms",
          1: "--",
          2: "80ms",
        }}
        testingLatency={false}
        queueingLatency={false}
        onToggleNode={vi.fn()}
        onToggleAll={vi.fn()}
        onPortChange={vi.fn()}
        onSaveRemark={vi.fn()}
        onTestNode={vi.fn()}
        onAnalyzeNode={vi.fn()}
        onDeleteSelected={vi.fn()}
        onTestSelected={vi.fn()}
        onTestAll={vi.fn()}
        {...overrides}
      />,
    );
  });

  return { container, root };
}

function getHeaderButton(container: HTMLElement, label: string) {
  const button = Array.from(container.querySelectorAll("th button")).find((item) =>
    item.textContent?.includes(label),
  );
  if (!button) {
    throw new Error(`Header button not found: ${label}`);
  }
  return button as HTMLButtonElement;
}

function getNodeOrder(container: HTMLElement) {
  return Array.from(container.querySelectorAll("tbody tr")).map((row) =>
    row.querySelector("td:nth-child(2)")?.textContent?.trim(),
  );
}

describe("NodeTable sorting", () => {
  it("sorts nodes by name in both directions", async () => {
    const { container, root } = await renderNodeTable();

    await act(async () => {
      getHeaderButton(container, "节点").click();
    });
    expect(getNodeOrder(container)).toEqual(["Alpha 01", "Bravo 02", "Charlie 03"]);

    await act(async () => {
      getHeaderButton(container, "节点").click();
    });
    expect(getNodeOrder(container)).toEqual(["Charlie 03", "Bravo 02", "Alpha 01"]);

    await act(async () => {
      getHeaderButton(container, "节点").click();
    });
    expect(getNodeOrder(container)).toEqual(["Charlie 03", "Alpha 01", "Bravo 02"]);

    await act(async () => {
      root.unmount();
    });
  });

  it("sorts assigned local ports before unassigned nodes", async () => {
    const { container, root } = await renderNodeTable();

    await act(async () => {
      getHeaderButton(container, "本地端口").click();
    });
    expect(getNodeOrder(container)).toEqual(["Bravo 02", "Charlie 03", "Alpha 01"]);

    await act(async () => {
      getHeaderButton(container, "本地端口").click();
    });
    expect(getNodeOrder(container)).toEqual(["Charlie 03", "Bravo 02", "Alpha 01"]);

    await act(async () => {
      getHeaderButton(container, "本地端口").click();
    });
    expect(getNodeOrder(container)).toEqual(["Charlie 03", "Alpha 01", "Bravo 02"]);

    await act(async () => {
      root.unmount();
    });
  });

  it("sorts latency by numeric delay and leaves missing values last", async () => {
    const { container, root } = await renderNodeTable();

    await act(async () => {
      getHeaderButton(container, "延迟").click();
    });
    expect(getNodeOrder(container)).toEqual(["Bravo 02", "Charlie 03", "Alpha 01"]);

    await act(async () => {
      getHeaderButton(container, "延迟").click();
    });
    expect(getNodeOrder(container)).toEqual(["Charlie 03", "Bravo 02", "Alpha 01"]);

    await act(async () => {
      getHeaderButton(container, "延迟").click();
    });
    expect(getNodeOrder(container)).toEqual(["Charlie 03", "Alpha 01", "Bravo 02"]);

    await act(async () => {
      root.unmount();
    });
  });
});

describe("NodeTable port editing", () => {
  it("clears an empty port draft without saving it", async () => {
    const onPortChange = vi.fn();
    const { container, root } = await renderNodeTable({ onPortChange });

    const portInput = Array.from(container.querySelectorAll("input")).find(
      (input) => input.value === "2080",
    ) as HTMLInputElement | undefined;
    expect(portInput).toBeDefined();

    await act(async () => {
      const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setValue?.call(portInput!, "");
      portInput!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      portInput!.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });

    const restoredInput = Array.from(container.querySelectorAll("input")).find(
      (input) => input.value === "2080",
    );
    expect(onPortChange).not.toHaveBeenCalled();
    expect(restoredInput).toBeDefined();

    await act(async () => {
      root.unmount();
    });
  });
});
