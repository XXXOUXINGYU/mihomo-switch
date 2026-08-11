import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { TopStatusBar } from "./TopStatusBar";

beforeAll(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
});

afterEach(() => {
  document.body.innerHTML = "";
});

describe("TopStatusBar", () => {
  it("announces pending mutations and restores core status afterward", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    const props = {
      running: true,
      mihomoExists: true,
      busy: true,
      runningPortCount: 2,
      invalidCount: 0,
      showWindowControls: false,
      maximized: false,
      onAddPort: vi.fn(),
      onToggleCore: vi.fn(),
      theme: "light" as const,
      onToggleTheme: vi.fn(),
      onInvalidClick: vi.fn(),
      onStartDrag: vi.fn(),
      onMinimize: vi.fn(),
      onToggleMaximize: vi.fn(),
      onHideToTray: vi.fn(),
    };

    await act(async () => root.render(<TopStatusBar {...props} />));
    const status = container.querySelector<HTMLElement>('[role="status"]');
    expect(status?.textContent).toContain("正在应用更改");
    expect(status?.querySelector(".spinner")).not.toBeNull();
    expect(container.querySelector<HTMLButtonElement>(".btnPrimarySoft")?.disabled).toBe(true);

    await act(async () => root.render(<TopStatusBar {...props} busy={false} />));
    expect(status?.textContent).toContain("核心运行中");
    expect(status?.querySelector(".spinner")).toBeNull();

    await act(async () => root.unmount());
  });
});
