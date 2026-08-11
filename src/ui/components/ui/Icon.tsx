import type { SVGProps } from "react";

import styles from "./Icon.module.css";

export type IconName =
  | "ports"
  | "nodes"
  | "subscriptions"
  | "activity"
  | "settings"
  | "sun"
  | "moon"
  | "search"
  | "close"
  | "plus"
  | "zap"
  | "pencil"
  | "trash"
  | "more"
  | "copy"
  | "alert"
  | "check"
  | "info"
  | "arrowUp"
  | "arrowDown"
  | "arrowLeft"
  | "arrowRight"
  | "minimize"
  | "maximize"
  | "restore"
  | "empty"
  | "inbox";

type Props = {
  name: IconName;
  size?: "sm" | "md" | "lg";
  className?: string;
};

type PathProps = SVGProps<SVGSVGElement>;

function Svg(props: PathProps) {
  return (
    <svg
      {...props}
      className={styles.svg}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    />
  );
}

const ICONS: Record<IconName, (props: PathProps) => JSX.Element> = {
  ports: (p) => (
    <Svg {...p}>
      <rect x="4" y="4" width="7" height="7" rx="1.5" />
      <rect x="13" y="4" width="7" height="7" rx="1.5" />
      <rect x="4" y="13" width="7" height="7" rx="1.5" />
      <rect x="13" y="13" width="7" height="7" rx="1.5" />
    </Svg>
  ),
  nodes: (p) => (
    <Svg {...p}>
      <path d="M12 4l7 4v8l-7 4-7-4V8z" />
    </Svg>
  ),
  subscriptions: (p) => (
    <Svg {...p}>
      <path d="M12 4v12" />
      <path d="M8 8l4-4 4 4" />
      <path d="M6 20h12" />
    </Svg>
  ),
  activity: (p) => (
    <Svg {...p}>
      <path d="M4 14c2-4 4-4 6 0s4 4 6 0 4-4 4-4" />
    </Svg>
  ),
  settings: (p) => (
    <Svg {...p}>
      <circle cx="12" cy="12" r="3" />
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    </Svg>
  ),
  sun: (p) => (
    <Svg {...p}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
    </Svg>
  ),
  moon: (p) => (
    <Svg {...p}>
      <path d="M20 14.5A8.5 8.5 0 0111.5 4 7 7 0 1020 14.5z" />
    </Svg>
  ),
  search: (p) => (
    <Svg {...p}>
      <circle cx="11" cy="11" r="6" />
      <path d="M16 16l4 4" />
    </Svg>
  ),
  close: (p) => (
    <Svg {...p}>
      <path d="M6 6l12 12M18 6L6 18" />
    </Svg>
  ),
  plus: (p) => (
    <Svg {...p}>
      <path d="M12 6v12M6 12h12" />
    </Svg>
  ),
  zap: (p) => (
    <Svg {...p}>
      <path d="M13 3L5 14h6l-1 7 8-11h-6z" />
    </Svg>
  ),
  pencil: (p) => (
    <Svg {...p}>
      <path d="M4 20h4l10-10-4-4L4 16z" />
      <path d="M14 6l4 4" />
    </Svg>
  ),
  trash: (p) => (
    <Svg {...p}>
      <path d="M4 7h16" />
      <path d="M9 7V5h6v2" />
      <path d="M8 7l1 12h6l1-12" />
    </Svg>
  ),
  more: (p) => (
    <Svg {...p}>
      <circle cx="6" cy="12" r="1.25" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.25" fill="currentColor" stroke="none" />
      <circle cx="18" cy="12" r="1.25" fill="currentColor" stroke="none" />
    </Svg>
  ),
  copy: (p) => (
    <Svg {...p}>
      <rect x="8" y="8" width="11" height="11" rx="1.5" />
      <path d="M6 16H5a2 2 0 01-2-2V5a2 2 0 012-2h9a2 2 0 012 2v1" />
    </Svg>
  ),
  alert: (p) => (
    <Svg {...p}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 8v5" />
      <circle cx="12" cy="16.5" r="0.75" fill="currentColor" stroke="none" />
    </Svg>
  ),
  check: (p) => (
    <Svg {...p}>
      <path d="M6 12.5l4 4 8-9" />
    </Svg>
  ),
  info: (p) => (
    <Svg {...p}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5" />
      <circle cx="12" cy="8" r="0.75" fill="currentColor" stroke="none" />
    </Svg>
  ),
  arrowUp: (p) => (
    <Svg {...p}>
      <path d="M12 18V6M7 11l5-5 5 5" />
    </Svg>
  ),
  arrowDown: (p) => (
    <Svg {...p}>
      <path d="M12 6v12M7 13l5 5 5-5" />
    </Svg>
  ),
  arrowLeft: (p) => (
    <Svg {...p}>
      <path d="M18 12H6M11 7l-5 5 5 5" />
    </Svg>
  ),
  arrowRight: (p) => (
    <Svg {...p}>
      <path d="M6 12h12M13 7l5 5-5 5" />
    </Svg>
  ),
  minimize: (p) => (
    <Svg {...p}>
      <path d="M6 12h12" />
    </Svg>
  ),
  maximize: (p) => (
    <Svg {...p}>
      <rect x="6" y="6" width="12" height="12" rx="1.5" />
    </Svg>
  ),
  restore: (p) => (
    <Svg {...p}>
      <rect x="8" y="8" width="10" height="10" rx="1.5" />
      <path d="M6 16V8a2 2 0 012-2h8" />
    </Svg>
  ),
  empty: (p) => (
    <Svg {...p}>
      <circle cx="12" cy="12" r="8" />
    </Svg>
  ),
  inbox: (p) => (
    <Svg {...p}>
      <path d="M4 8h16v10a2 2 0 01-2 2H6a2 2 0 01-2-2V8z" />
      <path d="M4 8l3-3h10l3 3" />
    </Svg>
  ),
};

export function Icon({ name, size = "md", className }: Props) {
  const Render = ICONS[name];
  return (
    <span className={[styles.icon, styles[size], className].filter(Boolean).join(" ")}>
      <Render />
    </span>
  );
}
