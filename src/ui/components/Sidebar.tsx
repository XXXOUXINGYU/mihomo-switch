import type { CSSProperties } from "react";
import { useEffect, useRef, useState } from "react";

import type { SubscriptionRecord } from "../types";
import styles from "./Sidebar.module.css";

type SlotRect = DOMRect;

type PointerDragState = {
  id: string;
  pointerId: number;
  startY: number;
  startIndex: number;
  startRect: SlotRect;
  engaged: boolean;
};

type DragOriginRect = {
  top: number;
  left: number;
  width: number;
  height: number;
};

type Props = {
  subscriptions: SubscriptionRecord[];
  currentSubId: string | null;
  activeSubCount: number;
  busy?: boolean;
  onSelect: (id: string) => void;
  onReorder: (orderedIds: string[]) => void;
};

export function Sidebar(props: Props) {
  const railRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);
  const itemRefs = useRef(new Map<string, HTMLDivElement>());
  const pointerDragRef = useRef<PointerDragState | null>(null);
  const subscriptionsRef = useRef(props.subscriptions);
  const reorderRef = useRef(props.onReorder);
  const suppressClickRef = useRef(false);
  const dragTargetIndexRef = useRef<number | null>(null);
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dragPointerY, setDragPointerY] = useState<number | null>(null);
  const [dragTargetIndex, setDragTargetIndex] = useState<number | null>(null);
  const [dragOriginRect, setDragOriginRect] = useState<DragOriginRect | null>(null);

  const updateRailState = () => {
    const rail = railRef.current;
    if (!rail) return;
    setCanScrollLeft(rail.scrollLeft > 8);
    setCanScrollRight(rail.scrollLeft + rail.clientWidth < rail.scrollWidth - 8);
  };

  const scrollRail = (direction: "left" | "right") => {
    const rail = railRef.current;
    if (!rail) return;
    rail.scrollBy({
      left: direction === "left" ? -220 : 220,
      behavior: "smooth",
    });
  };

  useEffect(() => {
    const rail = railRef.current;
    if (!rail) return;

    updateRailState();
    rail.addEventListener("scroll", updateRailState, { passive: true });
    window.addEventListener("resize", updateRailState);

    return () => {
      rail.removeEventListener("scroll", updateRailState);
      window.removeEventListener("resize", updateRailState);
    };
  }, [props.subscriptions.length]);

  useEffect(() => {
    const rail = railRef.current;
    if (!rail || !props.currentSubId) return;
    const active = rail.querySelector<HTMLElement>(`[data-sub-id="${props.currentSubId}"]`);
    active?.scrollIntoView({
      inline: "center",
      block: "nearest",
      behavior: "smooth",
    });
    updateRailState();
  }, [props.currentSubId]);

  useEffect(() => {
    subscriptionsRef.current = props.subscriptions;
  }, [props.subscriptions]);

  useEffect(() => {
    reorderRef.current = props.onReorder;
  }, [props.onReorder]);

  useEffect(() => {
    if (!draggingId) {
      setDragPointerY(null);
      setDragTargetIndex(null);
      setDragOriginRect(null);
    }
  }, [draggingId]);

  useEffect(() => {
    dragTargetIndexRef.current = dragTargetIndex;
  }, [dragTargetIndex]);

  const measureSlotRect = (element: HTMLDivElement): SlotRect => {
    const list = listRef.current;
    if (!list) {
      return element.getBoundingClientRect();
    }

    const listRect = list.getBoundingClientRect();
    let topOffset = 0;
    let leftOffset = 0;
    let current: HTMLElement | null = element;

    while (current && current !== list) {
      topOffset += current.offsetTop;
      leftOffset += current.offsetLeft;
      current = current.offsetParent as HTMLElement | null;
    }

    if (current !== list) {
      return element.getBoundingClientRect();
    }

    const top = listRect.top + topOffset - list.scrollTop;
    const left = listRect.left + leftOffset - list.scrollLeft;
    const width = element.offsetWidth;
    const height = element.offsetHeight;

    return {
      x: left,
      y: top,
      width,
      height,
      top,
      right: left + width,
      bottom: top + height,
      left,
      toJSON() {
        return this;
      },
    } as DOMRect;
  };

  const reorderIds = (orderedIds: string[], movingId: string, nextIndex: number) => {
    const fromIndex = orderedIds.indexOf(movingId);
    if (fromIndex < 0 || fromIndex === nextIndex) {
      return orderedIds;
    }

    const next = [...orderedIds];
    const [moved] = next.splice(fromIndex, 1);
    next.splice(nextIndex, 0, moved);
    return next;
  };

  const getReorderIndex = (orderedIds: string[], movingId: string, clientY: number) => {
    if (!orderedIds.includes(movingId)) {
      return -1;
    }

    const remainingIds = orderedIds.filter((id) => id !== movingId);
    for (const [index, candidateId] of remainingIds.entries()) {
      const element = itemRefs.current.get(candidateId);
      if (!element) {
        continue;
      }

      const rect = measureSlotRect(element);
      const midpoint = rect.top + rect.height / 2;
      if (clientY < midpoint) {
        return index;
      }
    }

    return remainingIds.length;
  };

  const resetDragState = () => {
    pointerDragRef.current = null;
    setDraggingId(null);
    setDragPointerY(null);
    setDragTargetIndex(null);
    document.body.style.removeProperty("user-select");
    document.body.style.removeProperty("cursor");
  };

  const commitReorder = () => {
    const drag = pointerDragRef.current;
    const currentOrder = subscriptionsRef.current.map((item) => item.id);
    const nextIndex = dragTargetIndexRef.current ?? drag?.startIndex ?? -1;
    const nextOrder =
      drag && nextIndex >= 0 ? reorderIds(currentOrder, drag.id, nextIndex) : currentOrder;
    const changed =
      nextOrder.length === currentOrder.length &&
      nextOrder.some((id, index) => id !== currentOrder[index]);

    resetDragState();

    if (changed) {
      reorderRef.current(nextOrder);
    }
  };

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const drag = pointerDragRef.current;
      if (!drag || event.pointerId !== drag.pointerId) {
        return;
      }

      if (!drag.engaged) {
        if (Math.abs(event.clientY - drag.startY) < 4) {
          return;
        }

        drag.engaged = true;
        suppressClickRef.current = true;
        document.body.style.setProperty("user-select", "none");
        document.body.style.setProperty("cursor", "grabbing");
        setDraggingId(drag.id);
        setDragTargetIndex(drag.startIndex);
        setDragOriginRect({
          top: drag.startRect.top,
          left: drag.startRect.left,
          width: drag.startRect.width,
          height: drag.startRect.height,
        });
      }

      event.preventDefault();
      setDragPointerY(event.clientY);
      const base = subscriptionsRef.current.map((sub) => sub.id);
      const nextIndex = getReorderIndex(base, drag.id, event.clientY);
      if (nextIndex >= 0) {
        setDragTargetIndex(nextIndex);
      }
    };

    const handlePointerFinish = (pointerId: number, commit: boolean) => {
      const drag = pointerDragRef.current;
      if (!drag || drag.pointerId !== pointerId) {
        return;
      }

      if (drag.engaged && commit) {
        commitReorder();
      } else {
        resetDragState();
      }
    };

    const handlePointerUp = (event: PointerEvent) => {
      handlePointerFinish(event.pointerId, true);
    };

    const handlePointerCancel = (event: PointerEvent) => {
      handlePointerFinish(event.pointerId, false);
    };

    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerCancel);

    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerCancel);
    };
  }, []);

  useEffect(
    () => () => {
      document.body.style.removeProperty("user-select");
      document.body.style.removeProperty("cursor");
    },
    [],
  );

  const getSlotShift = (fromIndex: number, toIndex: number) => {
    const fromId = props.subscriptions[fromIndex]?.id;
    const toId = props.subscriptions[toIndex]?.id;
    if (!fromId || !toId) {
      return 0;
    }

    const fromElement = itemRefs.current.get(fromId);
    const toElement = itemRefs.current.get(toId);
    if (!fromElement || !toElement) {
      return 0;
    }

    return measureSlotRect(toElement).top - measureSlotRect(fromElement).top;
  };

  const getItemStyle = (itemId: string, index: number) => {
    const drag = pointerDragRef.current;
    if (!drag) {
      return undefined;
    }

    if (itemId === drag.id) {
      return draggingId === drag.id
        ? ({ opacity: 0, pointerEvents: "none" } satisfies CSSProperties)
        : undefined;
    }

    const targetIndex = dragTargetIndex ?? drag.startIndex;
    if (targetIndex > drag.startIndex && index > drag.startIndex && index <= targetIndex) {
      const shift = getSlotShift(index, index - 1);
      return shift ? { transform: `translateY(${shift}px)` } : undefined;
    }

    if (targetIndex < drag.startIndex && index >= targetIndex && index < drag.startIndex) {
      const shift = getSlotShift(index, index + 1);
      return shift ? { transform: `translateY(${shift}px)` } : undefined;
    }

    return undefined;
  };

  const renderCard = (item: SubscriptionRecord) => (
    <div className={styles.card}>
      <div className={styles.headRow}>
        <div className={styles.icon}>{item.manual ? "手" : "订"}</div>
        <div className={styles.name} title={item.name}>{item.name}</div>
      </div>
      <div className={styles.foot}>
        <span className={styles.badge}>
          <span className={styles.badgeLabel}>启用</span>
          <span className={styles.badgeValue}>{item.selected_node_indices.length}</span>
        </span>
        <span className={styles.badgeMuted}>
          <span className={styles.badgeLabel}>节点</span>
          <span className={styles.badgeValue}>{item.nodes.length}</span>
        </span>
      </div>
    </div>
  );

  const dragItem =
    draggingId && dragOriginRect ? props.subscriptions.find((item) => item.id === draggingId) : null;
  const dragOverlayStyle: CSSProperties | undefined =
    dragItem && dragOriginRect && dragPointerY !== null && pointerDragRef.current
      ? {
          top: `${dragOriginRect.top + (dragPointerY - pointerDragRef.current.startY)}px`,
          left: `${dragOriginRect.left}px`,
          width: `${dragOriginRect.width}px`,
          height: `${dragOriginRect.height}px`,
        }
      : undefined;

  return (
    <section className={styles.wrap}>
      <div className={styles.header}>
        <h2 className={styles.title}>订阅面板</h2>
        <div className={styles.stats}>
          <div className={styles.stat}>
            <span className={styles.statValue}>{props.subscriptions.length}</span>
            <span className={styles.statLabel}>订阅数</span>
          </div>
          <div className={styles.stat}>
            <span className={styles.statValue}>{props.activeSubCount}</span>
            <span className={styles.statLabel}>启用中</span>
          </div>
        </div>
      </div>

      <div className={styles.railShell}>
        <button
          className={styles.railButton}
          onClick={() => scrollRail("left")}
          disabled={!canScrollLeft}
          aria-label="向左查看订阅"
        >
          ‹
        </button>
        <div className={styles.rail} ref={railRef}>
          {props.subscriptions.map((item) => {
            const selected = item.id === props.currentSubId;
            return (
              <button
                key={`rail-${item.id}`}
                type="button"
                data-sub-id={item.id}
                className={selected ? styles.railTabSelected : styles.railTab}
                onClick={() => props.onSelect(item.id)}
              >
                <span className={styles.railIcon}>{item.manual ? "手" : "订"}</span>
                <span className={styles.railMain}>
                  <span className={styles.railName}>{item.name}</span>
                  <span className={styles.railMeta}>
                    {item.selected_node_indices.length} 已启用 · {item.nodes.length} 节点
                  </span>
                </span>
              </button>
            );
          })}
        </div>
        <button
          className={styles.railButton}
          onClick={() => scrollRail("right")}
          disabled={!canScrollRight}
          aria-label="向右查看订阅"
        >
          ›
        </button>
      </div>

      <div className={styles.list} ref={listRef}>
        {props.subscriptions.map((item, index) => {
          const selected = item.id === props.currentSubId;
          const dragging = item.id === draggingId;
          const dragStyle = getItemStyle(item.id, index);

          return (
            <div
              key={item.id}
              data-sidebar-item={item.id}
              ref={(node) => {
                if (node) {
                  itemRefs.current.set(item.id, node);
                } else {
                  itemRefs.current.delete(item.id);
                }
              }}
              className={[
                selected ? styles.itemSelected : styles.item,
                dragging ? styles.itemDragging : "",
              ].filter(Boolean).join(" ")}
              style={dragStyle}
              onClick={(event) => {
                if (suppressClickRef.current) {
                  suppressClickRef.current = false;
                  event.preventDefault();
                  event.stopPropagation();
                  return;
                }

                props.onSelect(item.id);
              }}
              onPointerDown={(event) => {
                if (props.busy || event.button !== 0) return;
                event.preventDefault();
                suppressClickRef.current = false;
                const slotRect = measureSlotRect(event.currentTarget);
                pointerDragRef.current = {
                  id: item.id,
                  pointerId: event.pointerId,
                  startY: event.clientY,
                  startIndex: index,
                  startRect: slotRect,
                  engaged: false,
                };
              }}
            >
              {renderCard(item)}
            </div>
          );
        })}
        {dragItem && dragOverlayStyle ? (
          <div
            data-sidebar-ghost={dragItem.id}
            className={styles.dragGhost}
            style={dragOverlayStyle}
          >
            {renderCard(dragItem)}
          </div>
        ) : null}
      </div>
    </section>
  );
}
