import { Icon, type IconName } from "./ui/Icon";
import { useNavRailCollapsed } from "../hooks/useNavRailCollapsed";
import styles from "./NavRail.module.css";

export type PageId = "ports" | "nodes" | "subscriptions" | "activity" | "settings";

type NavItem = {
  id: PageId;
  label: string;
  icon: IconName;
};

const ITEMS: NavItem[] = [
  { id: "ports", label: "端口管理", icon: "ports" },
  { id: "nodes", label: "节点库", icon: "nodes" },
  { id: "subscriptions", label: "订阅管理", icon: "subscriptions" },
  { id: "activity", label: "连接活动", icon: "activity" },
  { id: "settings", label: "设置", icon: "settings" },
];

type Props = {
  active: PageId;
  onSelect: (page: PageId) => void;
  portCount: number;
  nodeCount: number;
  subscriptionCount: number;
  running: boolean;
  version: string;
};

export function NavRail(props: Props) {
  const { collapsed, toggleCollapsed } = useNavRailCollapsed();
  const badge: Partial<Record<PageId, number>> = {
    ports: props.portCount,
    nodes: props.nodeCount,
    subscriptions: props.subscriptionCount,
  };

  return (
    <nav
      className={styles.rail}
      data-collapsed={collapsed ? "true" : undefined}
      aria-label="主导航"
    >
      <ul className={styles.list}>
        {ITEMS.map((item) => {
          const active = props.active === item.id;
          const count = badge[item.id];
          return (
            <li key={item.id}>
              <button
                type="button"
                className={active ? styles.itemActive : styles.item}
                title={item.label}
                onClick={() => props.onSelect(item.id)}
                aria-current={active ? "page" : undefined}
              >
                <span className={styles.indicator} aria-hidden="true" />
                <span className={styles.icon} aria-hidden="true">
                  <Icon name={item.icon} size="md" />
                </span>
                <span className={styles.label}>{item.label}</span>
                {typeof count === "number" && count > 0 ? (
                  <span className={`mono ${styles.count}`}>{count}</span>
                ) : null}
              </button>
            </li>
          );
        })}
      </ul>

      <div className={styles.footer}>
        <div className={styles.footerStatus}>
          <div className={styles.coreState}>
            <span className={`dot ${props.running ? "dotLive" : "dotMuted"}`} aria-hidden="true" />
            <span className={styles.coreLabel}>{props.running ? "已连接核心" : "核心未运行"}</span>
          </div>
          <span className={`mono ${styles.version}`}>v{props.version}</span>
        </div>
        <button
          type="button"
          className={styles.collapseBtn}
          onClick={toggleCollapsed}
          title={collapsed ? "展开侧栏" : "收起侧栏"}
          aria-label={collapsed ? "展开侧栏" : "收起侧栏"}
          aria-expanded={!collapsed}
        >
          <Icon name={collapsed ? "arrowRight" : "arrowLeft"} size="sm" />
          <span className={styles.collapseLabel}>{collapsed ? "展开" : "收起侧栏"}</span>
        </button>
      </div>
    </nav>
  );
}
