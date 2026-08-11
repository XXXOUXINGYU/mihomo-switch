import { useCallback, useRef, useState } from "react";

export type ToastKind = "success" | "info" | "warn" | "error";

export type Toast = {
  id: number;
  kind: ToastKind;
  message: string;
};

export function useToast() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const idRef = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  const push = useCallback(
    (kind: ToastKind, message: string) => {
      const id = (idRef.current += 1);
      setToasts((prev) => [...prev.slice(-3), { id, kind, message }]);
      window.setTimeout(() => dismiss(id), kind === "error" ? 5200 : 2800);
    },
    [dismiss],
  );

  return { toasts, push, dismiss };
}
