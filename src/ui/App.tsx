import { useEffect, useMemo, useState } from "react";

import styles from "./App.module.css";
import packageInfo from "../../package.json";
import { NavRail, type PageId } from "./components/NavRail";
import { StartReviewDialog } from "./components/StartReviewDialog";
import { TopStatusBar } from "./components/TopStatusBar";
import { ToastStack } from "./components/ui/ToastStack";
import { Icon } from "./components/ui/Icon";
import { useAppData } from "./hooks/useAppData";
import { useTheme } from "./hooks/useTheme";
import { ActivityPage } from "./pages/ActivityPage";
import { NodeLibraryPage } from "./pages/NodeLibraryPage";
import { PortsPage } from "./pages/PortsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SubscriptionsPage } from "./pages/SubscriptionsPage";
import {
  hideWindowToTray,
  isWindowMaximized,
  minimizeWindow,
  startWindowDragging,
  toggleMaximizeWindow,
} from "./tauri";

const APP_VERSION = packageInfo.version;

export function App() {
  const app = useAppData();
  const { theme, toggleTheme } = useTheme();
  const [page, setPage] = useState<PageId>("ports");
  const [createPortRequested, setCreatePortRequested] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const [startReviewOpen, setStartReviewOpen] = useState(false);
  const [activityPort, setActivityPort] = useState<number | null>(null);

  const showWindowControls = !app.browserPreview;

  useEffect(() => {
    if (app.browserPreview) {
      return;
    }
    let active = true;
    const refresh = () => {
      void isWindowMaximized().then((value) => {
        if (active) {
          setMaximized(value);
        }
      });
    };
    refresh();
    window.addEventListener("resize", refresh);
    return () => {
      active = false;
      window.removeEventListener("resize", refresh);
    };
  }, [app.browserPreview]);

  const slots = app.payload?.slots ?? [];
  const settings = app.payload?.settings;
  const running = app.payload?.runtime.running ?? false;
  const mihomoExists = app.payload?.runtime.mihomo_exists ?? false;

  const runningPortCount = useMemo(
    () => slots.filter((slot) => slot.enabled && slot.state === "valid").length,
    [slots],
  );
  const invalidCount = useMemo(
    () => slots.filter((slot) => slot.enabled && (slot.state === "invalid" || slot.state === "unbound")).length,
    [slots],
  );
  const totalNodes = useMemo(
    () => (settings?.subscriptions ?? []).reduce((sum, sub) => sum + sub.nodes.length, 0),
    [settings?.subscriptions],
  );

  if (!app.payload) {
    if (app.bootstrapError) {
      return (
        <div className={styles.loadFailure} role="alert">
          <span className={styles.loadFailureIcon} aria-hidden="true">
            <Icon name="alert" size="md" />
          </span>
          <h1 className={styles.loadFailureTitle}>应用启动失败</h1>
          <p className={styles.loadFailureMessage}>{app.bootstrapError}</p>
          <button className="btn btnPrimary" disabled={app.bootstrapping} onClick={() => void app.retryBootstrap()}>
            {app.bootstrapping ? "正在重试…" : "重新加载"}
          </button>
        </div>
      );
    }
    return (
      <div className={styles.loading}>
        <span className="spinner" aria-hidden="true" />
        <span>正在加载…</span>
      </div>
    );
  }

  const runnableSlots = slots.filter((slot) => slot.enabled && slot.state === "valid");
  const blockedSlots = slots.filter((slot) => slot.enabled && slot.state !== "valid");

  const handleToggleCore = () => {
    if (running) {
      void app.stopCore();
      return;
    }
    // Concentrate "cannot start" warnings before launching the core.
    if (blockedSlots.length > 0) {
      setStartReviewOpen(true);
      return;
    }
    void app.startCore();
  };

  const renderPage = () => {
    switch (page) {
      case "ports":
        return (
          <PortsPage
            app={app}
            createRequested={createPortRequested}
            onCreateRequestHandled={() => setCreatePortRequested(false)}
            onOpenActivity={(port) => {
              setActivityPort(port);
              setPage("activity");
            }}
          />
        );
      case "nodes":
        return <NodeLibraryPage app={app} />;
      case "subscriptions":
        return <SubscriptionsPage app={app} />;
      case "activity":
        return (
          <ActivityPage
            app={app}
            focusPort={activityPort}
            onClearFocus={() => setActivityPort(null)}
          />
        );
      case "settings":
        return <SettingsPage app={app} />;
      default:
        return null;
    }
  };

  return (
    <div className={styles.shell}>
      <TopStatusBar
        running={running}
        mihomoExists={mihomoExists}
        busy={app.busy}
        runningPortCount={running ? runningPortCount : 0}
        invalidCount={invalidCount}
        showWindowControls={showWindowControls}
        maximized={maximized}
        onAddPort={() => {
          setPage("ports");
          setCreatePortRequested(true);
        }}
        onToggleCore={handleToggleCore}
        theme={theme}
        onToggleTheme={toggleTheme}
        onInvalidClick={() => setPage("ports")}
        onStartDrag={() => {
          if (!app.browserPreview) {
            void startWindowDragging();
          }
        }}
        onMinimize={() => void minimizeWindow()}
        onToggleMaximize={() => void toggleMaximizeWindow()}
        onHideToTray={() => void hideWindowToTray()}
      />

      <div className={styles.body}>
        <NavRail
          active={page}
          onSelect={(nextPage) => {
            if (nextPage === "activity") {
              setActivityPort(null);
            }
            setPage(nextPage);
          }}
          portCount={slots.length}
          nodeCount={totalNodes}
          subscriptionCount={settings?.subscriptions.length ?? 0}
          running={running}
          version={APP_VERSION}
        />
        <main className={styles.workspace}>{renderPage()}</main>
      </div>

      <StartReviewDialog
        open={startReviewOpen}
        busy={app.busy}
        runnable={runnableSlots}
        blocked={blockedSlots}
        onCancel={() => setStartReviewOpen(false)}
        onConfirm={async () => {
          const ok = await app.startCore();
          if (ok) {
            setStartReviewOpen(false);
          }
        }}
      />

      <ToastStack toasts={app.toasts} onDismiss={app.dismissToast} />
    </div>
  );
}
