import { useMemo, useState } from "react";

import type { AppData } from "../hooks/useAppData";
import type { SubscriptionRecord, UpsertSubscriptionInput } from "../types";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { SubscriptionDialog } from "../components/SubscriptionDialog";
import { EmptyState } from "../components/ui/EmptyState";
import { Icon } from "../components/ui/Icon";
import styles from "./common.module.css";

type Props = {
  app: AppData;
};

export function SubscriptionsPage({ app }: Props) {
  const settings = app.payload?.settings;
  const subscriptions = settings?.subscriptions ?? [];
  const slots = settings?.port_slots ?? [];

  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogMode, setDialogMode] = useState<"create" | "edit">("create");
  const [editing, setEditing] = useState<SubscriptionRecord | null>(null);
  const [deleting, setDeleting] = useState<SubscriptionRecord | null>(null);

  const affectedCount = useMemo(() => {
    if (!deleting) return 0;
    return slots.filter((slot) => slot.binding?.sub_id === deleting.id).length;
  }, [deleting, slots]);
  const initialValue = useMemo(
    () =>
      dialogMode === "edit" && editing
        ? { name: editing.name, url: editing.url, manual: editing.manual, content: editing.content }
        : null,
    [dialogMode, editing],
  );

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <div className={styles.headingGroup}>
          <h1 className={styles.title}>订阅管理</h1>
          <p className={styles.subtitle}>管理订阅来源。更新订阅会尽量保留端口的节点绑定，失效的绑定会在端口页提示重绑。</p>
        </div>
        <button className="btn btnPrimarySoft" onClick={() => { setDialogMode("create"); setEditing(null); setDialogOpen(true); }}>
          <Icon name="plus" size="sm" /> 新增订阅
        </button>
      </header>

      <div className={styles.scroll}>
        {subscriptions.length === 0 ? (
          <EmptyState
            icon={<Icon name="subscriptions" size="lg" />}
            title="还没有订阅"
            description="添加一个 URL 订阅或手动粘贴节点内容，之后即可把节点绑定到端口。"
            action={<button className="btn btnPrimary" onClick={() => { setDialogMode("create"); setEditing(null); setDialogOpen(true); }}>新增订阅</button>}
          />
        ) : (
          <div className={styles.cardList}>
            {subscriptions.map((sub) => {
              const used = slots.filter((slot) => slot.binding?.sub_id === sub.id).length;
              return (
                <div key={sub.id} className={styles.card}>
                  <div className={styles.cardMain}>
                    <span className={styles.cardTitle}>{sub.name}</span>
                    <span className={styles.cardMeta}>
                      <span>{sub.nodes.length} 个节点</span>
                      <span className={styles.metaDivider}>·</span>
                      <span>{sub.manual ? "手动来源" : "URL 订阅"}</span>
                      <span className={styles.metaDivider}>·</span>
                      <span>{used} 个端口在用</span>
                    </span>
                  </div>
                  <div className={styles.cardActions}>
                    {!sub.manual ? (
                      <button className="btn btnSm" disabled={app.busy} onClick={() => void app.refreshSubscription(sub.id)}>
                        更新
                      </button>
                    ) : null}
                    <button className="btn btnSm btnGhost" onClick={() => { setDialogMode("edit"); setEditing(sub); setDialogOpen(true); }}>
                      编辑
                    </button>
                    <button className="btn btnSm btnDanger" onClick={() => setDeleting(sub)}>
                      删除
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <SubscriptionDialog
        open={dialogOpen}
        mode={dialogMode}
        busy={app.busy}
        initialValue={initialValue}
        onClose={() => setDialogOpen(false)}
        onSubmit={async (input: UpsertSubscriptionInput) => {
          const ok =
            dialogMode === "edit" && editing
              ? await app.editSubscription(editing.id, input)
              : await app.addSubscription(input);
          if (ok) {
            setDialogOpen(false);
          }
        }}
      />

      <ConfirmDialog
        open={Boolean(deleting)}
        busy={app.busy}
        title="删除订阅"
        message={
          deleting
            ? affectedCount > 0
              ? `删除「${deleting.name}」后，${affectedCount} 个端口的节点绑定将失效，需要重新选择节点。`
              : `确定要删除订阅「${deleting.name}」吗？`
            : ""
        }
        confirmLabel="删除订阅"
        onClose={() => setDeleting(null)}
        onConfirm={async () => {
          if (deleting) {
            const ok = await app.removeSubscription(deleting.id);
            if (ok) {
              setDeleting(null);
            }
          }
        }}
      />
    </div>
  );
}
