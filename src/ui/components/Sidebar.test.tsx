import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { SubscriptionRecord } from "../types";
import { Sidebar } from "./Sidebar";

const subscriptions: SubscriptionRecord[] = [
  {
    id: "sub-a",
    name: "Office Relay",
    url: "",
    ua: "mihomo-switch",
    start_port: 7890,
    manual: false,
    content: "",
    nodes: [{ name: "A-1", type: "vmess", server: "a.example.dev", port: 443 }],
    selected_node_indices: [0],
    port_assignments: { "0": 7891 },
    node_remarks: {},
  },
  {
    id: "sub-b",
    name: "Mobile Backup",
    url: "",
    ua: "mihomo-switch",
    start_port: 7900,
    manual: true,
    content: "",
    nodes: [{ name: "B-1", type: "vless", server: "b.example.dev", port: 8443 }],
    selected_node_indices: [],
    port_assignments: {},
    node_remarks: {},
  },
  {
    id: "sub-c",
    name: "Lab Nodes",
    url: "",
    ua: "mihomo-switch",
    start_port: 7910,
    manual: false,
    content: "",
    nodes: [{ name: "C-1", type: "trojan", server: "c.example.dev", port: 9443 }],
    selected_node_indices: [],
    port_assignments: {},
    node_remarks: {},
  },
];

type RenderResult = {
  container: HTMLDivElement;
  root: Root;
  onSelect: ReturnType<typeof vi.fn>;
  onReorder: ReturnType<typeof vi.fn>;
  rectOverrides: WeakMap<HTMLElement, DOMRect>;
};

beforeAll(() => {
  Object.assign(globalThis, {
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  if (!("PointerEvent" in window)) {
    Object.defineProperty(window, "PointerEvent", {
      configurable: true,
      value: MouseEvent,
    });
  }
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: vi.fn(),
  });
});

afterEach(() => {
  document.body.innerHTML = "";
});

function buildRect(top: number): DOMRect {
  return {
    x: 0,
    y: top,
    width: 180,
    height: 80,
    top,
    right: 180,
    bottom: top + 80,
    left: 0,
    toJSON() {
      return this;
    },
  } as DOMRect;
}

function dispatchPointerEvent(
  target: Element | Window,
  type: "pointerdown" | "pointermove" | "pointerup" | "pointercancel",
  clientY: number,
  pointerId = 1,
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clientY", { configurable: true, value: clientY });
  Object.defineProperty(event, "pointerId", { configurable: true, value: pointerId });
  Object.defineProperty(event, "button", { configurable: true, value: 0 });
  target.dispatchEvent(event);
}

function getCards(container: HTMLElement) {
  return Array.from(container.querySelectorAll<HTMLElement>("[data-sidebar-item]"));
}

function installLayoutMetrics(container: HTMLElement, rectOverrides: WeakMap<HTMLElement, DOMRect>) {
  const items = getCards(container);
  const list = items[0]?.parentElement as HTMLElement | null;
  if (!list) {
    throw new Error("Sidebar list not found");
  }

  Object.defineProperty(list, "getBoundingClientRect", {
    configurable: true,
    value: () => ({
      ...buildRect(100),
      width: 220,
      right: 220,
      bottom: 420,
    }),
  });
  Object.defineProperty(list, "scrollTop", {
    configurable: true,
    get: () => 0,
  });
  Object.defineProperty(list, "scrollLeft", {
    configurable: true,
    get: () => 0,
  });

  for (const item of items) {
    Object.defineProperty(item, "offsetParent", {
      configurable: true,
      get: () => list,
    });
    Object.defineProperty(item, "offsetTop", {
      configurable: true,
      get: () => {
        const currentItems = getCards(container);
        const index = currentItems.indexOf(item);
        return Math.max(index, 0) * 90;
      },
    });
    Object.defineProperty(item, "offsetLeft", {
      configurable: true,
      get: () => 0,
    });
    Object.defineProperty(item, "offsetWidth", {
      configurable: true,
      get: () => 180,
    });
    Object.defineProperty(item, "offsetHeight", {
      configurable: true,
      get: () => 80,
    });
    Object.defineProperty(item, "getBoundingClientRect", {
      configurable: true,
      value: () => {
        const override = rectOverrides.get(item);
        if (override) {
          return override;
        }

        const currentItems = getCards(container);
        const index = currentItems.indexOf(item);
        return buildRect(100 + Math.max(index, 0) * 90);
      },
    });
  }
}

function installNestedOffsetMetrics(
  container: HTMLElement,
  rectOverrides: WeakMap<HTMLElement, DOMRect>,
) {
  const items = getCards(container);
  const list = items[0]?.parentElement as HTMLElement | null;
  if (!list) {
    throw new Error("Sidebar list not found");
  }

  const wrapper = document.createElement("div");

  Object.defineProperty(list, "getBoundingClientRect", {
    configurable: true,
    value: () => ({
      ...buildRect(100),
      width: 220,
      right: 220,
      bottom: 420,
    }),
  });
  Object.defineProperty(list, "scrollTop", {
    configurable: true,
    get: () => 0,
  });
  Object.defineProperty(list, "scrollLeft", {
    configurable: true,
    get: () => 0,
  });

  Object.defineProperty(wrapper, "offsetParent", {
    configurable: true,
    get: () => list,
  });
  Object.defineProperty(wrapper, "offsetTop", {
    configurable: true,
    get: () => 8,
  });
  Object.defineProperty(wrapper, "offsetLeft", {
    configurable: true,
    get: () => 10,
  });

  for (const [index, item] of items.entries()) {
    Object.defineProperty(item, "offsetParent", {
      configurable: true,
      get: () => wrapper,
    });
    Object.defineProperty(item, "offsetTop", {
      configurable: true,
      get: () => index * 90,
    });
    Object.defineProperty(item, "offsetLeft", {
      configurable: true,
      get: () => 0,
    });
    Object.defineProperty(item, "offsetWidth", {
      configurable: true,
      get: () => 180,
    });
    Object.defineProperty(item, "offsetHeight", {
      configurable: true,
      get: () => 80,
    });
    Object.defineProperty(item, "getBoundingClientRect", {
      configurable: true,
      value: () => {
        const override = rectOverrides.get(item);
        if (override) {
          return override;
        }

        return buildRect(108 + index * 90);
      },
    });
  }
}

function getOrder(container: HTMLElement) {
  return getCards(container).map((item) => item.textContent?.replace(/\s+/g, " ").trim());
}

function getTransforms(container: HTMLElement) {
  return getCards(container).map((item) => item.style.transform || "");
}

function getGhost(container: HTMLElement) {
  return container.querySelector<HTMLElement>("[data-sidebar-ghost]");
}

async function renderSidebar(): Promise<RenderResult> {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  const onSelect = vi.fn();
  const onReorder = vi.fn();
  const rectOverrides = new WeakMap<HTMLElement, DOMRect>();

  await act(async () => {
    root.render(
      <Sidebar
        subscriptions={subscriptions}
        currentSubId="sub-a"
        activeSubCount={1}
        onSelect={onSelect}
        onReorder={onReorder}
      />,
    );
  });

  installLayoutMetrics(container, rectOverrides);
  return { container, root, onSelect, onReorder, rectOverrides };
}

describe("Sidebar drag reorder", () => {
  it("keeps plain click selection working", async () => {
    const { container, root, onSelect } = await renderSidebar();
    const items = getCards(container);

    await act(async () => {
      items[1].click();
    });

    expect(onSelect).toHaveBeenCalledWith("sub-b");

    await act(async () => {
      root.unmount();
    });
  });

  it("keeps rendering while dragging down and back up without dropping", async () => {
    const { container, root, onReorder } = await renderSidebar();
    const items = getCards(container);

    await act(async () => {
      dispatchPointerEvent(items[0], "pointerdown", 120);
    });
    expect(getOrder(container)).toEqual([
      "订Office Relay启用1节点1",
      "手Mobile Backup启用0节点1",
      "订Lab Nodes启用0节点1",
    ]);

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 380);
    });
    expect(getOrder(container)).toEqual([
      "订Office Relay启用1节点1",
      "手Mobile Backup启用0节点1",
      "订Lab Nodes启用0节点1",
    ]);
    expect(getTransforms(container)).toEqual(["", "translateY(-90px)", "translateY(-90px)"]);
    expect(getGhost(container)?.getAttribute("data-sidebar-ghost")).toBe("sub-a");

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 110);
    });
    expect(getOrder(container)).toEqual([
      "订Office Relay启用1节点1",
      "手Mobile Backup启用0节点1",
      "订Lab Nodes启用0节点1",
    ]);
    expect(getTransforms(container)).toEqual(["", "", ""]);
    expect(getGhost(container)?.getAttribute("data-sidebar-ghost")).toBe("sub-a");

    await act(async () => {
      dispatchPointerEvent(window, "pointerup", 110);
    });
    expect(getOrder(container)).toEqual([
      "订Office Relay启用1节点1",
      "手Mobile Backup启用0节点1",
      "订Lab Nodes启用0节点1",
    ]);
    expect(getTransforms(container)).toEqual(["", "", ""]);
    expect(getGhost(container)).toBeNull();
    expect(onReorder).not.toHaveBeenCalled();

    await act(async () => {
      root.unmount();
    });
  });

  it("positions the drag ghost in viewport coordinates while dragging", async () => {
    const { container, root } = await renderSidebar();
    const items = getCards(container);

    await act(async () => {
      dispatchPointerEvent(items[0], "pointerdown", 120);
    });

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 236);
    });

    const ghost = getGhost(container);
    expect(ghost).not.toBeNull();
    expect(ghost?.style.top).toBe("216px");
    expect(ghost?.style.left).toBe("0px");
    expect(ghost?.style.width).toBe("180px");
    expect(ghost?.style.height).toBe("80px");

    await act(async () => {
      dispatchPointerEvent(window, "pointerup", 236);
      root.unmount();
    });
  });

  it("shifts the next card as soon as dragging downward crosses its midpoint", async () => {
    const { container, root, onReorder } = await renderSidebar();
    const items = getCards(container);

    await act(async () => {
      dispatchPointerEvent(items[0], "pointerdown", 120);
    });

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 236);
    });

    expect(getOrder(container)).toEqual([
      "订Office Relay启用1节点1",
      "手Mobile Backup启用0节点1",
      "订Lab Nodes启用0节点1",
    ]);
    expect(getTransforms(container)).toEqual(["", "translateY(-90px)", ""]);
    expect(getGhost(container)?.getAttribute("data-sidebar-ghost")).toBe("sub-a");
    expect(onReorder).not.toHaveBeenCalled();

    await act(async () => {
      dispatchPointerEvent(window, "pointerup", 236);
    });

    expect(onReorder).toHaveBeenCalledWith(["sub-b", "sub-a", "sub-c"]);

    await act(async () => {
      root.unmount();
    });
  });

  it("commits the preview order on drop", async () => {
    const { container, root, onReorder } = await renderSidebar();
    const items = getCards(container);

    await act(async () => {
      dispatchPointerEvent(items[0], "pointerdown", 120);
    });

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 380);
    });
    expect(getTransforms(container)).toEqual(["", "translateY(-90px)", "translateY(-90px)"]);
    expect(getGhost(container)?.getAttribute("data-sidebar-ghost")).toBe("sub-a");

    await act(async () => {
      dispatchPointerEvent(window, "pointerup", 380);
    });

    expect(onReorder).toHaveBeenCalledWith(["sub-b", "sub-c", "sub-a"]);
    expect(getTransforms(container)).toEqual(["", "", ""]);
    expect(getGhost(container)).toBeNull();

    await act(async () => {
      root.unmount();
    });
  });

  it("uses stable slot geometry when reversing through visually shifted cards", async () => {
    const { container, root, rectOverrides } = await renderSidebar();
    const items = getCards(container);

    await act(async () => {
      dispatchPointerEvent(items[0], "pointerdown", 120);
    });

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 380);
    });
    expect(getTransforms(container)).toEqual(["", "translateY(-90px)", "translateY(-90px)"]);
    expect(getGhost(container)?.getAttribute("data-sidebar-ghost")).toBe("sub-a");

    await act(async () => {
      rectOverrides.set(items[1], buildRect(-320));
      rectOverrides.set(items[2], buildRect(-230));
      dispatchPointerEvent(window, "pointermove", 110);
      rectOverrides.delete(items[1]);
      rectOverrides.delete(items[2]);
    });

    expect(getOrder(container)).toEqual([
      "订Office Relay启用1节点1",
      "手Mobile Backup启用0节点1",
      "订Lab Nodes启用0节点1",
    ]);
    expect(getTransforms(container)).toEqual(["", "", ""]);

    await act(async () => {
      dispatchPointerEvent(window, "pointerup", 110);
      root.unmount();
    });
  });

  it("reorders correctly when card offsets are nested under a different offset parent", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    const onSelect = vi.fn();
    const onReorder = vi.fn();
    const rectOverrides = new WeakMap<HTMLElement, DOMRect>();

    await act(async () => {
      root.render(
        <Sidebar
          subscriptions={subscriptions}
          currentSubId="sub-a"
          activeSubCount={1}
          onSelect={onSelect}
          onReorder={onReorder}
        />,
      );
    });

    installNestedOffsetMetrics(container, rectOverrides);
    const items = getCards(container);

    await act(async () => {
      dispatchPointerEvent(items[0], "pointerdown", 120);
    });

    await act(async () => {
      dispatchPointerEvent(window, "pointermove", 250);
    });

    expect(getTransforms(container)).toEqual(["", "translateY(-90px)", ""]);
    expect(getGhost(container)?.getAttribute("data-sidebar-ghost")).toBe("sub-a");

    await act(async () => {
      dispatchPointerEvent(window, "pointerup", 250);
    });

    expect(onReorder).toHaveBeenCalledWith(["sub-b", "sub-a", "sub-c"]);

    await act(async () => {
      root.unmount();
    });
  });
});
