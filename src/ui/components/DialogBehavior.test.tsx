import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { SubscriptionRecord } from "../types";
import { ConfirmDialog } from "./ConfirmDialog";
import { NodePicker } from "./NodePicker";

beforeAll(() => {
  Object.assign(globalThis, {
    IS_REACT_ACT_ENVIRONMENT: true,
  });
});

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllTimers();
  vi.useRealTimers();
});

function mount() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  return { container, root: createRoot(container) };
}

function buttonByText(container: HTMLElement, text: string) {
  const button = Array.from(container.querySelectorAll("button")).find(
    (item) => item.textContent === text,
  );
  if (!button) throw new Error(`Button not found: ${text}`);
  return button as HTMLButtonElement;
}

describe("dialog dismissal behavior", () => {
  it("moves focus into a dialog, traps Tab, and restores focus after closing", async () => {
    vi.useFakeTimers();
    const { container, root } = mount();
    const opener = document.createElement("button");
    opener.textContent = "打开";
    document.body.prepend(opener);
    opener.focus();

    await act(async () => {
      root.render(
        <ConfirmDialog
          open
          busy={false}
          title="确认操作"
          message="请确认。"
          confirmLabel="确认"
          onClose={vi.fn()}
          onConfirm={vi.fn()}
        />,
      );
    });
    await act(async () => {
      vi.runAllTimers();
    });

    const cancel = buttonByText(container, "取消");
    const confirm = buttonByText(container, "确认");
    expect(document.activeElement).toBe(cancel);

    confirm.focus();
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    });
    expect(document.activeElement).toBe(cancel);

    cancel.focus();
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true }));
    });
    expect(document.activeElement).toBe(confirm);

    await act(async () => root.render(<ConfirmDialog open={false} busy={false} title="" message="" confirmLabel="" onClose={vi.fn()} onConfirm={vi.fn()} />));
    expect(document.activeElement).toBe(opener);

    await act(async () => root.unmount());
  });

  it("keeps a parent dialog open when dismissing a nested node picker", async () => {
    vi.useFakeTimers();
    const { container, root } = mount();
    const onParentClose = vi.fn();
    const onCancel = vi.fn();
    const subscriptions: SubscriptionRecord[] = [];

    await act(async () => {
      root.render(
        <div onClick={onParentClose}>
          <NodePicker
            open
            portName="测试端口"
            localPort={10808}
            subscriptions={subscriptions}
            currentNodeName={null}
            current={null}
            latencyMap={{}}
            onTest={vi.fn()}
            onCancel={onCancel}
            onConfirm={vi.fn()}
          />
        </div>,
      );
      vi.runAllTimers();
    });

    const overlay = container.querySelector<HTMLElement>(".overlay");
    expect(overlay).not.toBeNull();
    await act(async () => {
      overlay?.click();
    });

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onParentClose).not.toHaveBeenCalled();

    await act(async () => root.unmount());
  });

  it("cannot be dismissed while a destructive action is busy", async () => {
    const { container, root } = mount();
    const onClose = vi.fn();

    await act(async () => {
      root.render(
        <ConfirmDialog
          open
          busy
          title="删除订阅"
          message="删除后无法恢复。"
          confirmLabel="确认删除"
          onClose={onClose}
          onConfirm={vi.fn()}
        />,
      );
    });

    const overlay = container.querySelector<HTMLElement>(".overlay");
    await act(async () => {
      overlay?.click();
      buttonByText(container, "取消").click();
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });

    expect(buttonByText(container, "取消").disabled).toBe(true);
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => root.unmount());
  });
});
