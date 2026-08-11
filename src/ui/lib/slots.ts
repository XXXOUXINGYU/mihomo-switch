import type {
  AppSettings,
  NodeBinding,
  PortSlotView,
  ProxyNode,
  SubscriptionRecord,
} from "../types";

export function nodeFingerprint(node: Pick<ProxyNode, "type" | "server" | "port">): string {
  return `${node.type}|${node.server}|${node.port}`;
}

export function resolveBinding(
  sub: SubscriptionRecord,
  binding: NodeBinding,
): { index: number; node: ProxyNode } | null {
  let fallback: { index: number; node: ProxyNode } | null = null;
  for (let index = 0; index < sub.nodes.length; index += 1) {
    const node = sub.nodes[index];
    if (nodeFingerprint(node) !== binding.fingerprint) {
      continue;
    }
    if (node.name === binding.node_name) {
      return { index, node };
    }
    if (!fallback) {
      fallback = { index, node };
    }
  }
  return fallback;
}

/// Client-side mirror of the Rust `build_slot_views`, used in browser preview
/// where there is no backend to resolve bindings.
export function buildSlotViews(settings: AppSettings): PortSlotView[] {
  return settings.port_slots.map((slot) => {
    if (!slot.binding) {
      return {
        id: slot.id,
        name: slot.name,
        note: slot.note,
        local_port: slot.local_port,
        enabled: slot.enabled,
        state: "unbound",
        invalid_reason: null,
        binding: null,
      };
    }

    const sub = settings.subscriptions.find((item) => item.id === slot.binding?.sub_id) ?? null;
    if (!sub) {
      return {
        id: slot.id,
        name: slot.name,
        note: slot.note,
        local_port: slot.local_port,
        enabled: slot.enabled,
        state: "invalid",
        invalid_reason: "所属订阅已删除",
        binding: {
          sub_id: slot.binding.sub_id,
          sub_name: "未知订阅",
          node_index: null,
          node_name: slot.binding.node_name,
          node_type: "",
          server: "",
          server_port: 0,
        },
      };
    }

    const resolved = resolveBinding(sub, slot.binding);
    if (!resolved) {
      return {
        id: slot.id,
        name: slot.name,
        note: slot.note,
        local_port: slot.local_port,
        enabled: slot.enabled,
        state: "invalid",
        invalid_reason: "节点已失效",
        binding: {
          sub_id: sub.id,
          sub_name: sub.name,
          node_index: null,
          node_name: slot.binding.node_name,
          node_type: "",
          server: "",
          server_port: 0,
        },
      };
    }

    return {
      id: slot.id,
      name: slot.name,
      note: slot.note,
      local_port: slot.local_port,
      enabled: slot.enabled,
      state: "valid",
      invalid_reason: null,
      binding: {
        sub_id: sub.id,
        sub_name: sub.name,
        node_index: resolved.index,
        node_name: resolved.node.name,
        node_type: resolved.node.type,
        server: resolved.node.server,
        server_port: resolved.node.port,
      },
    };
  });
}
