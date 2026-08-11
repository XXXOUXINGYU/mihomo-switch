import { useCallback, useEffect, useState } from "react";

import { applyTheme, persistTheme, readStoredTheme, type Theme } from "../theme/theme";

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => readStoredTheme());

  useEffect(() => {
    applyTheme(theme);
    persistTheme(theme);
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => (current === "light" ? "dark" : "light"));
  }, []);

  return { theme, toggleTheme, setTheme };
}
