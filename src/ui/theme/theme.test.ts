import { describe, expect, it, vi } from "vitest";

import { applyTheme, persistTheme, readStoredTheme } from "./theme";

describe("theme", () => {
  it("persists and restores the selected theme", () => {
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => {
        storage.set(key, value);
      },
    });

    persistTheme("dark");
    expect(readStoredTheme()).toBe("dark");
    applyTheme(readStoredTheme());
    expect(document.documentElement.dataset.theme).toBe("dark");

    vi.unstubAllGlobals();
  });
});
