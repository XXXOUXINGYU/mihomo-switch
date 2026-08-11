import { useEffect, useMemo, useRef, useState } from "react";

import type { SubscriptionRecord } from "../types";
import type { LatencyMap } from "../hooks/useAppData";
import { latencyKey } from "../hooks/useAppData";
import { useDialogFocus } from "../hooks/useDialogFocus";
import { useEscapeKey } from "../hooks/useEscapeKey";
import { detectRegion } from "../lib/region";
import { latencyMs, latencyTone, protocolLabel } from "../lib/format";
import { Icon } from "./ui/Icon";
import styles from "./NodePicker.module.css";

export type NodeChoice = { subId: string; nodeIndex: number };

type FlatNode = {
  subId: string;
  subName: string;
  nodeIndex: number;
  name: string;
  type: string;
  server: string;
  region: string;
  flag: string;
};

type Props = {
  open: boolean;
  title?: string;
  portName: string;
  localPort: number;
  subscriptions: SubscriptionRecord[];
  currentNodeName: string | null;
  current: NodeChoice | null;
  latencyMap: LatencyMap;
  onTest: (subId: string, nodeIndex: number) => void;
  onCancel: () => void;
  onConfirm: (choice: NodeChoice) => void;
};

type SortKey = "default" | "latency" | "name";

const toneClass: Record<string, string> = {
  good: styles.latGood,
  warn: styles.latWarn,
  bad: styles.latBad,
  pending: styles.latPending,
  idle: styles.latIdle,
};

export function NodePicker(props: Props) {
  const [keyword, setKeyword] = useState("");
  const [subFilter, setSubFilter] = useState<string>("all");
  const [regionFilter, setRegionFilter] = useState<string>("all");
  const [sort, setSort] = useState<SortKey>("default");
  const [onlyAvailable, setOnlyAvailable] = useState(false);
  const [selected, setSelected] = useState<NodeChoice | null>(props.current);
  const searchRef = useRef<HTMLInputElement>(null);
  const dialogRef = useDialogFocus(props.open, searchRef);

  useEscapeKey(props.open, props.onCancel);

  useEffect(() => {
    if (props.open) {
      setSelected(props.current);
      setKeyword("");
      setSubFilter("all");
      setRegionFilter("all");
      setSort("default");
      setOnlyAvailable(false);
      window.setTimeout(() => searchRef.current?.focus(), 30);
    }
  }, [props.open, props.current]);

  const allNodes = useMemo<FlatNode[]>(() => {
    const list: FlatNode[] = [];
    for (const sub of props.subscriptions) {
      sub.nodes.forEach((node, nodeIndex) => {
        const region = detectRegion(node.name);
        list.push({
          subId: sub.id,
          subName: sub.name,
          nodeIndex,
          name: node.name,
          type: node.type,
          server: node.server,
          region: region.label,
          flag: region.flag,
        });
      });
    }
    return list;
  }, [props.subscriptions]);

  const regions = useMemo(
    () => Array.from(new Set(allNodes.map((node) => node.region))),
    [allNodes],
  );

  const visible = useMemo(() => {
    const lower = keyword.trim().toLowerCase();
    const filtered = allNodes.filter((node) => {
      if (subFilter !== "all" && node.subId !== subFilter) return false;
      if (regionFilter !== "all" && node.region !== regionFilter) return false;
      if (lower && !`${node.name} ${node.server} ${node.type}`.toLowerCase().includes(lower)) {
        return false;
      }
      if (onlyAvailable) {
        const tone = latencyTone(props.latencyMap[latencyKey(node.subId, node.nodeIndex)]);
        if (tone !== "good" && tone !== "warn") return false;
      }
      return true;
    });

    if (sort === "name") {
      return [...filtered].sort((a, b) => a.name.localeCompare(b.name, "zh-CN", { numeric: true }));
    }
    if (sort === "latency") {
      return [...filtered].sort((a, b) => {
        const la = latencyMs(props.latencyMap[latencyKey(a.subId, a.nodeIndex)]);
        const lb = latencyMs(props.latencyMap[latencyKey(b.subId, b.nodeIndex)]);
        if (la === null && lb === null) return 0;
        if (la === null) return 1;
        if (lb === null) return -1;
        return la - lb;
      });
    }
    return filtered;
  }, [allNodes, keyword, subFilter, regionFilter, onlyAvailable, sort, props.latencyMap]);

  if (!props.open) {
    return null;
  }

  const selectedNode = selected
    ? allNodes.find((node) => node.subId === selected.subId && node.nodeIndex === selected.nodeIndex)
    : null;

  return (
    <div
      className="overlay"
      onClick={(event) => {
        event.stopPropagation();
        props.onCancel();
      }}
    >
      <div
        ref={dialogRef}
        className={`dialog dialogWide ${styles.dialog}`}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label={props.title ?? "选择节点"}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialogHeader">
          <h2 className="dialogTitle">{props.title ?? "选择节点"}</h2>
          <p className="dialogSubtitle">
            为端口「{props.portName}」（<span className="mono">127.0.0.1:{props.localPort}</span>）选择绑定节点。端口号保持不变。
          </p>
        </div>

        <div className={styles.toolbar}>
          <input
            ref={searchRef}
            className={`input ${styles.search}`}
            placeholder="搜索节点名称、地址或协议…"
            aria-label="搜索节点"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
          />
          <div className={styles.filters}>
            <select aria-label="按订阅筛选" className={`select ${styles.select}`} value={subFilter} onChange={(event) => setSubFilter(event.target.value)}>
              <option value="all">全部订阅</option>
              {props.subscriptions.map((sub) => (
                <option key={sub.id} value={sub.id}>{sub.name}</option>
              ))}
            </select>
            <select aria-label="按地区筛选" className={`select ${styles.select}`} value={regionFilter} onChange={(event) => setRegionFilter(event.target.value)}>
              <option value="all">全部地区</option>
              {regions.map((region) => (
                <option key={region} value={region}>{region}</option>
              ))}
            </select>
            <select aria-label="节点排序" className={`select ${styles.select}`} value={sort} onChange={(event) => setSort(event.target.value as SortKey)}>
              <option value="default">默认排序</option>
              <option value="latency">按延迟</option>
              <option value="name">按名称</option>
            </select>
            <label className={styles.onlyAvailable}>
              <input type="checkbox" checked={onlyAvailable} onChange={(event) => setOnlyAvailable(event.target.checked)} />
              只显示可用
            </label>
          </div>
        </div>

        <div className={styles.listWrap}>
          {visible.length === 0 ? (
            <div className={styles.empty}>没有符合条件的节点。</div>
          ) : (
            <ul className={styles.list}>
              {visible.map((node) => {
                const key = latencyKey(node.subId, node.nodeIndex);
                const latency = props.latencyMap[key] ?? "--";
                const tone = latencyTone(latency);
                const isSelected =
                  selected?.subId === node.subId && selected?.nodeIndex === node.nodeIndex;
                const isCurrent =
                  props.current?.subId === node.subId && props.current?.nodeIndex === node.nodeIndex;
                return (
                  <li key={key}>
                    <div
                      className={isSelected ? styles.rowSelected : styles.row}
                      role="button"
                      tabIndex={0}
                      onClick={() => setSelected({ subId: node.subId, nodeIndex: node.nodeIndex })}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          setSelected({ subId: node.subId, nodeIndex: node.nodeIndex });
                        }
                      }}
                    >
                      <span className={styles.flag} aria-hidden="true">{node.flag}</span>
                      <span className={styles.nodeMain}>
                        <span className={styles.nodeName}>
                          {node.name}
                          {isCurrent ? <span className={styles.currentBadge}>当前</span> : null}
                        </span>
                        <span className={styles.nodeMeta}>
                          {node.subName} · {protocolLabel(node.type)} · {node.region}
                        </span>
                      </span>
                      <span className={`mono ${styles.latency} ${toneClass[tone]}`}>{latency}</span>
                      <button
                        type="button"
                        className="btn btnSm"
                        onClick={(event) => {
                          event.stopPropagation();
                          props.onTest(node.subId, node.nodeIndex);
                        }}
                      >
                        测速
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        <div className={`dialogFooter ${styles.footer}`}>
          <div className={styles.preview}>
            {selectedNode ? (
              <>
                <span className={styles.previewLabel}>新映射</span>
                <span className="mono">127.0.0.1:{props.localPort}</span>
                <span className={styles.arrow} aria-hidden="true"><Icon name="arrowRight" size="sm" /></span>
                <span className={styles.previewNode}>{selectedNode.flag} {selectedNode.name}</span>
                {props.currentNodeName && props.currentNodeName !== selectedNode.name ? (
                  <span className={styles.previewOld}>原：{props.currentNodeName}</span>
                ) : null}
              </>
            ) : (
              <span className={styles.previewLabel}>请选择一个节点</span>
            )}
          </div>
          <div className={styles.footerActions}>
            <button className="btn" onClick={props.onCancel}>取消</button>
            <button
              className="btn btnPrimary"
              disabled={!selected}
              onClick={() => selected && props.onConfirm(selected)}
            >
              确认绑定
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
