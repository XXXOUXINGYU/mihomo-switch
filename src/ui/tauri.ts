import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type {
  BootstrapPayload,
  LatencyResult,
  LogEntry,
  NodeTrafficPanel,
  NodeTrafficSnapshot,
  PortSlotBatchBindingInput,
  PortSlotBindingInput,
  PortSlotInput,
  PortTrafficReport,
  PortValidation,
  UpsertSubscriptionInput,
} from "./types";

export const bootstrap = () => invoke<BootstrapPayload>("bootstrap");

export const createPortSlot = (input: PortSlotInput) =>
  invoke<BootstrapPayload>("create_port_slot", { input });

export const updatePortSlot = (slotId: string, input: PortSlotInput) =>
  invoke<BootstrapPayload>("update_port_slot", { slotId, input });

export const deletePortSlot = (slotId: string) =>
  invoke<BootstrapPayload>("delete_port_slot", { slotId });

export const setPortSlotEnabled = (slotId: string, enabled: boolean) =>
  invoke<BootstrapPayload>("set_port_slot_enabled", { slotId, enabled });

export const bindPortSlot = (slotId: string, binding: PortSlotBindingInput) =>
  invoke<BootstrapPayload>("bind_port_slot", { slotId, binding });

export const bindPortSlotsBatch = (assignments: PortSlotBatchBindingInput[]) =>
  invoke<BootstrapPayload>("bind_port_slots_batch", { assignments });

export const clearPortSlotBinding = (slotId: string) =>
  invoke<BootstrapPayload>("clear_port_slot_binding", { slotId });

export const reorderPortSlots = (orderedIds: string[]) =>
  invoke<BootstrapPayload>("reorder_port_slots", { orderedIds });

export const validatePort = (port: number, ignoreSlotId: string | null) =>
  invoke<PortValidation>("validate_port", { port, ignoreSlotId });

export const getPortTraffic = () => invoke<PortTrafficReport>("port_traffic");

export const createSubscription = (input: UpsertSubscriptionInput) =>
  invoke<BootstrapPayload>("create_subscription", { input });

export const updateSubscription = (subId: string, input: UpsertSubscriptionInput) =>
  invoke<BootstrapPayload>("update_subscription", { subId, input });

export const deleteSubscription = (subId: string) =>
  invoke<BootstrapPayload>("delete_subscription", { subId });

export const deleteSelectedNodes = (subId: string, nodeIndices: number[]) =>
  invoke<BootstrapPayload>("delete_selected_nodes", { subId, nodeIndices });

export const reorderSubscriptions = (orderedIds: string[]) =>
  invoke<BootstrapPayload>("reorder_subscriptions", { orderedIds });

export const importSubscription = (subId: string) =>
  invoke<BootstrapPayload>("import_subscription", { subId });

export const saveSelection = (
  subId: string,
  selectedNodeIndices: number[],
  portAssignments: Record<string, number>,
) =>
  invoke<BootstrapPayload>("save_selection", {
    subId,
    selectedNodeIndices,
    portAssignments,
  });

export const saveNodeRemark = (subId: string, nodeIndex: number, remark: string) =>
  invoke<BootstrapPayload>("save_node_remark", {
    subId,
    nodeIndex,
    remark,
  });

export const saveSubconverter = (url: string) =>
  invoke<BootstrapPayload>("save_subconverter", { url });

export const saveProxySettings = (enabled: boolean, url: string, mihomoPath: string) =>
  invoke<BootstrapPayload>("save_proxy_settings", { enabled, url, mihomoPath });

export const startMihomo = () => invoke<BootstrapPayload>("start_mihomo");

export const stopMihomo = () => invoke<BootstrapPayload>("stop_mihomo");

export const cancelLatency = () => invoke<void>("cancel_latency");

export const testLatency = (subId: string, nodeIndices: number[]) =>
  invoke<LatencyResult[]>("test_latency", { subId, nodeIndices });

export const getNodeTrafficSnapshot = (subId: string, nodeIndex: number) =>
  invoke<NodeTrafficSnapshot>("node_traffic_snapshot", { subId, nodeIndex });

export const getNodeTrafficPanel = (subId: string, nodeIndex: number) =>
  invoke<NodeTrafficPanel>("node_traffic_panel", { subId, nodeIndex });

export const listenLogs = (handler: (entry: LogEntry) => void): Promise<UnlistenFn> =>
  listen<LogEntry>("runtime-log", (event) => handler(event.payload));

export const listenLatencyResults = (
  handler: (result: LatencyResult) => void,
): Promise<UnlistenFn> =>
  listen<LatencyResult>("latency-result", (event) => handler(event.payload));

export async function minimizeWindow() {
  const window = getCurrentWindow();
  await window.minimize();
}

export async function hideWindowToTray() {
  const window = getCurrentWindow();
  await window.hide();
}

export async function toggleMaximizeWindow() {
  const window = getCurrentWindow();
  await window.toggleMaximize();
}

export async function isWindowMaximized() {
  const window = getCurrentWindow();
  return window.isMaximized();
}

export async function startWindowDragging() {
  const window = getCurrentWindow();
  await window.startDragging();
}
