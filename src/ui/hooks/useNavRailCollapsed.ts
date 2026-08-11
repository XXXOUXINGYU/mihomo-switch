import { useCallback, useEffect, useState } from "react";

const STORAGE_KEY = "mihomo-switch-nav-collapsed";

function readStoredCollapsed() {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function useNavRailCollapsed() {
  const [collapsed, setCollapsed] = useState(() => readStoredCollapsed());

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, collapsed ? "1" : "0");
    } catch {
      /* ignore storage errors */
    }
  }, [collapsed]);

  const toggleCollapsed = useCallback(() => {
    setCollapsed((current) => !current);
  }, []);

  return { collapsed, toggleCollapsed };
}
