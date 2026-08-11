import { useEffect, useMemo, useState } from "react";

import type { SubscriptionRecord } from "../types";
import styles from "./NodeTable.module.css";

type Props = {
  subscription: SubscriptionRecord | null;
  latencyMap: Record<number, string>;
  testingLatency: boolean;
  queueingLatency: boolean;
  onToggleNode: (index: number, checked: boolean) => void;
  onToggleAll: (visibleIndices: number[], checked: boolean) => void;
  onPortChange: (index: number, port: number) => void;
  onSaveRemark: (index: number, remark: string) => void;
  onTestNode: (index: number) => void;
  onAnalyzeNode: (index: number) => void;
  onDeleteSelected: () => void;
  onTestSelected: () => void;
  onTestAll: () => void;
};

type SortKey = "node" | "localPort" | "latency";
type SortDirection = "asc" | "desc";

type SortState = {
  key: SortKey;
  direction: SortDirection;
} | null;

type VisibleNode = {
  node: SubscriptionRecord["nodes"][number];
  index: number;
};

function getLatencyClass(latency: string) {
  if (latency === "测试中..." || latency === "排队中...") {
    return styles.latencyPending;
  }

  if (latency === "失败" || latency === "超时" || latency === "已暂停") {
    return styles.latencyBad;
  }

  if (!latency.endsWith("ms")) {
    return styles.latencyIdle;
  }

  const value = Number.parseInt(latency, 10);
  if (!Number.isFinite(value)) {
    return styles.latencyIdle;
  }

  if (value <= 150) {
    return styles.latencyGood;
  }

  if (value <= 300) {
    return styles.latencyWarn;
  }

  return styles.latencyBad;
}

function parseLatencyValue(latency: string) {
  if (!latency.endsWith("ms")) {
    return null;
  }

  const value = Number.parseInt(latency, 10);
  return Number.isFinite(value) ? value : null;
}

export function NodeTable(props: Props) {
  const [keyword, setKeyword] = useState("");
  const [portDrafts, setPortDrafts] = useState<Record<string, string>>({});
  const [remarkDrafts, setRemarkDrafts] = useState<Record<string, string>>({});
  const [menu, setMenu] = useState<{ x: number; y: number; index: number } | null>(null);
  const [sort, setSort] = useState<SortState>(null);

  useEffect(() => {
    if (!menu) {
      return;
    }

    const close = () => setMenu(null);
    const handleKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };

    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", handleKeydown);
    };
  }, [menu]);

  const visible = useMemo(() => {
    const subscription = props.subscription;
    if (!subscription) return [];
    const lower = keyword.toLowerCase().trim();
    const filtered = subscription.nodes
      .map((node, index) => ({ node, index }))
      .filter(({ node, index }) => {
        if (!lower) return true;
        const remark = subscription.node_remarks[String(index)] ?? "";
        return `${node.name} ${node.server} ${node.type} ${remark}`.toLowerCase().includes(lower);
      });

    if (!sort) {
      return filtered;
    }

    const directionFactor = sort.direction === "asc" ? 1 : -1;
    const compareWithFallback = (left: VisibleNode, right: VisibleNode, result: number) =>
      result === 0 ? left.index - right.index : result;

    return [...filtered].sort((left, right) => {
      if (sort.key === "node") {
        return compareWithFallback(
          left,
          right,
          left.node.name.localeCompare(right.node.name, "zh-CN", {
            numeric: true,
            sensitivity: "base",
          }) * directionFactor,
        );
      }

      if (sort.key === "localPort") {
        const leftPort = subscription.port_assignments[String(left.index)];
        const rightPort = subscription.port_assignments[String(right.index)];
        const leftAssigned = Number.isFinite(leftPort);
        const rightAssigned = Number.isFinite(rightPort);

        if (leftAssigned !== rightAssigned) {
          return leftAssigned ? -1 : 1;
        }

        if (leftAssigned && rightAssigned) {
          return compareWithFallback(left, right, (leftPort - rightPort) * directionFactor);
        }

        return left.index - right.index;
      }

      const leftLatency = parseLatencyValue(props.latencyMap[left.index] ?? "--");
      const rightLatency = parseLatencyValue(props.latencyMap[right.index] ?? "--");
      const leftHasLatency = leftLatency !== null;
      const rightHasLatency = rightLatency !== null;

      if (leftHasLatency !== rightHasLatency) {
        return leftHasLatency ? -1 : 1;
      }

      if (leftHasLatency && rightHasLatency) {
        return compareWithFallback(left, right, (leftLatency - rightLatency) * directionFactor);
      }

      return left.index - right.index;
    });
  }, [props.subscription, keyword, props.latencyMap, sort]);

  const toggleSort = (key: SortKey) => {
    setSort((prev) => {
      if (prev?.key === key) {
        if (prev.direction === "desc") {
          return null;
        }

        return {
          key,
          direction: "desc",
        };
      }
      return {
        key,
        direction: "asc",
      };
    });
  };

  const renderSortableHeader = (key: SortKey, label: string) => {
    const active = sort?.key === key;
    const direction = active ? sort.direction : null;
    return (
      <button
        type="button"
        className={active ? styles.sortButtonActive : styles.sortButton}
        onClick={() => toggleSort(key)}
        aria-label={`${label}排序`}
      >
        <span>{label}</span>
        <span className={styles.sortIcon}>{direction === "desc" ? "↓" : "↑"}</span>
      </button>
    );
  };

  const commitPort = (index: number) => {
    const draft = portDrafts[String(index)];
    if (draft === undefined) return;
    const trimmed = draft.trim();
    const nextPort = Number.parseInt(trimmed, 10);
    setPortDrafts((prev) => {
      const next = { ...prev };
      delete next[String(index)];
      return next;
    });
    if (!trimmed || !Number.isFinite(nextPort) || nextPort < 1 || nextPort > 65535) {
      return;
    }
    props.onPortChange(index, nextPort);
  };

  const commitRemark = (index: number) => {
    if (!props.subscription) return;
    const key = String(index);
    const draft = remarkDrafts[key];
    const current = props.subscription.node_remarks[key] ?? "";
    const nextRemark = (draft ?? current).trim();
    setRemarkDrafts((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
    if (nextRemark === current) {
      return;
    }
    props.onSaveRemark(index, nextRemark);
  };

  if (!props.subscription) {
    return <div className={styles.empty}>先创建或选择一个订阅，再开始导入和筛选节点。</div>;
  }

  const selectedSet = new Set(props.subscription.selected_node_indices);
  const allVisibleSelected =
    visible.length > 0 && visible.every((item) => selectedSet.has(item.index));

  return (
    <section className={styles.wrap}>
      <div className={styles.header}>
        <div className={styles.toolbar}>
          <input
            className={styles.search}
            placeholder="搜索节点、备注、地址或协议..."
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
          />
          <div className={styles.actions}>
            <button
              className={styles.actionButton}
              disabled={
                !props.subscription ||
                props.testingLatency ||
                props.queueingLatency ||
                props.subscription.selected_node_indices.length === 0
              }
              onClick={props.onDeleteSelected}
            >
              删选中
            </button>
            <button
              className={styles.actionButton}
              disabled={
                !props.subscription ||
                props.testingLatency ||
                props.queueingLatency ||
                props.subscription.selected_node_indices.length === 0
              }
              onClick={props.onTestSelected}
            >
              测选中
            </button>
            <button
              className={styles.actionButton}
              disabled={!props.subscription || props.testingLatency || props.queueingLatency}
              onClick={props.onTestAll}
            >
              测全部
            </button>
          </div>
        </div>
      </div>

      <div className={styles.tableWrap}>
        <table className={styles.table}>
          <colgroup>
            <col className={styles.colSelect} />
            <col className={styles.colNode} />
            <col className={styles.colRemark} />
            <col className={styles.colProtocol} />
            <col className={styles.colRemote} />
            <col className={styles.colLocal} />
            <col className={styles.colLatency} />
          </colgroup>
          <thead>
            <tr>
              <th className={styles.selectHead}>
                <label className={styles.headCheckbox}>
                  <input
                    type="checkbox"
                    aria-label="全选筛选结果"
                    checked={allVisibleSelected}
                    onChange={(event) =>
                      props.onToggleAll(
                        visible.map((item) => item.index),
                        event.target.checked,
                      )
                    }
                  />
                </label>
              </th>
              <th aria-sort={sort?.key === "node" ? (sort.direction === "asc" ? "ascending" : "descending") : "none"}>
                {renderSortableHeader("node", "节点")}
              </th>
              <th>备注</th>
              <th>协议</th>
              <th>远端</th>
              <th aria-sort={sort?.key === "localPort" ? (sort.direction === "asc" ? "ascending" : "descending") : "none"}>
                {renderSortableHeader("localPort", "本地端口")}
              </th>
              <th aria-sort={sort?.key === "latency" ? (sort.direction === "asc" ? "ascending" : "descending") : "none"}>
                {renderSortableHeader("latency", "延迟")}
              </th>
            </tr>
          </thead>
          <tbody>
            {visible.map(({ node, index }) => {
              const checked = selectedSet.has(index);
              const currentPort = props.subscription?.port_assignments[String(index)] ?? "";
              const currentRemark = props.subscription?.node_remarks[String(index)] ?? "";
              const latency = props.latencyMap[index] ?? "--";
              const latencyClass = getLatencyClass(latency);
              return (
                <tr
                  key={`${node.name}-${index}`}
                  className={checked ? styles.rowSelected : undefined}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    setMenu({
                      x: event.clientX,
                      y: event.clientY,
                      index,
                    });
                  }}
                >
                  <td className={styles.checkboxCell}>
                    <input
                      className={styles.checkboxInput}
                      type="checkbox"
                      checked={checked}
                      onChange={(event) => props.onToggleNode(index, event.target.checked)}
                    />
                  </td>
                  <td>
                    <button
                      type="button"
                      className={styles.nodeButton}
                      title="点击测速"
                      disabled={props.testingLatency}
                      onClick={() => props.onTestNode(index)}
                    >
                      <span className={styles.nodeText} title={node.name}>
                        {node.name}
                      </span>
                    </button>
                  </td>
                  <td>
                    <input
                      className={styles.remarkInput}
                      placeholder="添加备注"
                      value={remarkDrafts[String(index)] ?? currentRemark}
                      onChange={(event) =>
                        setRemarkDrafts((prev) => ({
                          ...prev,
                          [String(index)]: event.target.value,
                        }))
                      }
                      onBlur={() => commitRemark(index)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.currentTarget.blur();
                        }
                      }}
                    />
                  </td>
                  <td>
                    <span className={styles.protocol}>{node.type.toUpperCase()}</span>
                  </td>
                  <td className={styles.mono}>{node.port}</td>
                  <td className={styles.mono}>
                    {checked ? (
                      <input
                        className={styles.portInput}
                        value={portDrafts[String(index)] ?? String(currentPort)}
                        onChange={(event) =>
                          setPortDrafts((prev) => ({
                            ...prev,
                            [String(index)]: event.target.value,
                          }))
                        }
                        onBlur={() => commitPort(index)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.currentTarget.blur();
                          }
                        }}
                      />
                    ) : (
                      "--"
                    )}
                  </td>
                  <td className={`${styles.mono} ${latencyClass}`}>{latency}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {menu ? (
        <div
          className={styles.menu}
          style={{ left: menu.x, top: menu.y }}
          onClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            className={styles.menuItem}
            onClick={() => {
              props.onAnalyzeNode(menu.index);
              setMenu(null);
            }}
          >
            流量分析
          </button>
        </div>
      ) : null}
    </section>
  );
}
