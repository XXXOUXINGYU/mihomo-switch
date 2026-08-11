import { useEffect } from "react";

/// Invoke `onEscape` whenever the Escape key is pressed while `active` is true.
/// Used by dialogs/overlays so they can be dismissed from the keyboard.
export function useEscapeKey(active: boolean, onEscape: () => void) {
  useEffect(() => {
    if (!active) {
      return;
    }
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onEscape();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [active, onEscape]);
}
