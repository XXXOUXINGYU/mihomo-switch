export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const fixed = value >= 100 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(2);
  return `${fixed} ${units[unitIndex]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) {
    return "0 B/s";
  }
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function proxyAddress(port: number): string {
  return `127.0.0.1:${port}`;
}

export type LatencyTone = "good" | "warn" | "bad" | "pending" | "idle";

export function latencyTone(value: string | undefined): LatencyTone {
  if (!value || value === "--") {
    return "idle";
  }
  if (value === "测试中..." || value === "排队中...") {
    return "pending";
  }
  if (value === "失败" || value === "超时" || value === "已暂停") {
    return "bad";
  }
  if (!value.endsWith("ms")) {
    return "idle";
  }
  const ms = Number.parseInt(value, 10);
  if (!Number.isFinite(ms)) {
    return "idle";
  }
  if (ms <= 150) {
    return "good";
  }
  if (ms <= 300) {
    return "warn";
  }
  return "bad";
}

export function latencyMs(value: string | undefined): number | null {
  if (!value || !value.endsWith("ms")) {
    return null;
  }
  const ms = Number.parseInt(value, 10);
  return Number.isFinite(ms) ? ms : null;
}

export function protocolLabel(type: string): string {
  const normalized = type.trim().toLowerCase();
  const map: Record<string, string> = {
    ss: "Shadowsocks",
    ssr: "ShadowsocksR",
    vmess: "VMess",
    vless: "VLESS",
    trojan: "Trojan",
    hysteria: "Hysteria",
    hysteria2: "Hysteria2",
    tuic: "TUIC",
    socks5: "SOCKS5",
    http: "HTTP",
  };
  return map[normalized] ?? type.toUpperCase();
}
