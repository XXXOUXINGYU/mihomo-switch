import type {
  AppSettings,
  BootstrapPayload,
  PortSlotView,
  PortTrafficReport,
  ProxyNode,
  SubscriptionRecord,
} from "../types";
import { buildSlotViews, nodeFingerprint } from "./slots";

function node(name: string, type: string, server: string, port: number): ProxyNode {
  return { name, type, server, port };
}

const personalNodes: ProxyNode[] = [
  node("日本 - 东京 01", "ss", "jp-tokyo-01.example.dev", 443),
  node("新加坡 - 01", "vmess", "sg-01.example.dev", 443),
  node("美国 - 洛杉矶 02", "vless", "us-la-02.example.dev", 8443),
  node("德国 - 法兰克福 01", "trojan", "de-fra-01.example.dev", 443),
  node("香港 - 02", "ss", "hk-02.example.dev", 443),
  node("台湾 - 台北 01", "vmess", "tw-taipei-01.example.dev", 443),
  node("英国 - 伦敦 01", "vless", "uk-ldn-01.example.dev", 443),
];

const backupNodes: ProxyNode[] = [
  node("香港 - IEPL 01", "trojan", "hk-iepl-01.example.dev", 443),
  node("日本 - 大阪 01", "vmess", "jp-osaka-01.example.dev", 2087),
  node("美国 - 圣何塞 01", "vless", "us-sjc-01.example.dev", 8443),
];

const companyNodes: ProxyNode[] = [
  node("新加坡 - 专线 01", "trojan", "sg-pl-01.example.dev", 443),
  node("韩国 - 首尔 01", "vmess", "kr-seoul-01.example.dev", 443),
];

function subscription(
  id: string,
  name: string,
  nodes: ProxyNode[],
  startPort: number,
): SubscriptionRecord {
  return {
    id,
    name,
    url: `https://${id}.example.dev/sub`,
    ua: "mihomo-switch",
    start_port: startPort,
    manual: false,
    content: "",
    nodes,
    selected_node_indices: [],
    port_assignments: {},
    node_remarks: {},
  };
}

export function createPreviewPayload(): BootstrapPayload {
  const personal = subscription("preview-personal", "个人订阅", personalNodes, 10801);
  const backup = subscription("preview-backup", "备用订阅", backupNodes, 10901);
  const company = subscription("preview-company", "公司订阅", companyNodes, 11001);
  const test = subscription("preview-test", "测试订阅", [], 11101);

  const bindTo = (sub: SubscriptionRecord, index: number) => ({
    sub_id: sub.id,
    fingerprint: nodeFingerprint(sub.nodes[index]),
    node_name: sub.nodes[index].name,
  });

  const settings: AppSettings = {
    schema_version: 4,
    subscriptions: [personal, backup, company, test],
    slots_migrated: true,
    subconverter: "https://sub.example.dev",
    local_proxy_enabled: false,
    local_proxy_url: "http://127.0.0.1:20122",
    mihomo_path: "C:/Users/preview/.mihomo_switch/mihomo.exe",
    port_slots: [
      {
        id: "slot-us",
        name: "美区主力",
        note: "AdsPower 窗口 12",
        local_port: 10808,
        enabled: true,
        binding: bindTo(personal, 2),
      },
      {
        id: "slot-jp",
        name: "日本账号 A",
        note: "指纹 #4 · 流媒体",
        local_port: 10809,
        enabled: true,
        binding: bindTo(personal, 0),
      },
      {
        id: "slot-hk",
        name: "香港日常",
        note: "AdsPower 窗口 03",
        local_port: 10810,
        enabled: true,
        binding: bindTo(personal, 4),
      },
      {
        id: "slot-sg",
        name: "新加坡测试",
        note: "暂停使用",
        local_port: 10811,
        enabled: false,
        binding: bindTo(company, 0),
      },
      {
        id: "slot-broken",
        name: "旧美区节点",
        note: "订阅更新后节点消失",
        local_port: 10812,
        enabled: true,
        binding: {
          sub_id: personal.id,
          fingerprint: "vless|us-sjc-legacy.example.dev|8443",
          node_name: "美国 - 圣何塞 99",
        },
      },
      {
        id: "slot-empty",
        name: "待分配端口",
        note: "新建窗口预留",
        local_port: 10813,
        enabled: true,
        binding: null,
      },
    ],
  };

  return {
    settings,
    slots: buildSlotViews(settings),
    runtime: {
      config_path: "E:/preview/runtime/pool_config.yaml",
      mihomo_path: "E:/preview/mihomo/mihomo.exe",
      mihomo_exists: true,
      runtime_dir: "E:/preview/runtime",
      running: true,
    },
    migration: null,
  };
}

/// Deterministic per-slot preview metrics so the table shows realistic latency,
/// connections, and traffic without a backend.
export function previewSlotMetrics(localPort: number) {
  const seed = localPort % 97;
  const latencyMs = 42 + (seed % 9) * 13;
  return {
    latency: `${latencyMs} ms`,
    connections: seed % 6,
    upload: 120_000_000 + seed * 7_300_000,
    download: 680_000_000 + seed * 41_000_000,
  };
}

const PREVIEW_HOSTS = [
  "graph.microsoft.com",
  "api.openai.com",
  "www.googleapis.com",
  "cdn.jsdelivr.net",
  "telegram.org",
  "youtube.com",
  "github.com",
];

/// Build a synthetic per-port traffic report for browser preview, so the table,
/// status panel, and activity page all show coherent live-looking data.
export function buildPreviewTraffic(
  settings: AppSettings,
  slots: PortSlotView[],
): PortTrafficReport {
  const ports = [];
  const connections = [];
  for (const slot of slots) {
    if (!slot.enabled || slot.state !== "valid") {
      continue;
    }
    const metrics = previewSlotMetrics(slot.local_port);
    ports.push({
      local_port: slot.local_port,
      upload: metrics.upload,
      download: metrics.download,
      upload_speed: 40_000 + (slot.local_port % 50) * 5_000,
      download_speed: 320_000 + (slot.local_port % 50) * 22_000,
      connections: metrics.connections,
    });
    for (let i = 0; i < metrics.connections; i += 1) {
      const host = PREVIEW_HOSTS[(slot.local_port + i) % PREVIEW_HOSTS.length];
      connections.push({
        id: `${slot.id}-${i}`,
        local_port: slot.local_port,
        host,
        destination: `${host}:443`,
        rule: "GeoLocation-!CN",
        chain: slot.binding?.node_name ?? "",
        process: ["chrome.exe", "Code.exe", "msedge.exe"][(slot.local_port + i) % 3],
        network: "tcp",
        upload: 1_200_000 + i * 320_000,
        download: 8_400_000 + i * 1_700_000,
        upload_speed: 12_000 + i * 3_000,
        download_speed: 96_000 + i * 24_000,
      });
    }
  }
  void settings;
  return {
    running: true,
    sampled_at: new Date().toISOString(),
    message: `已捕获 ${connections.length} 条活动连接`,
    ports,
    connections,
  };
}
