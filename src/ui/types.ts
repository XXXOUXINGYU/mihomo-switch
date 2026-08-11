export type ProxyNode = {
  name: string;
  type: string;
  server: string;
  port: number;
  uuid?: string;
  alterId?: number;
  cipher?: string;
  udp?: boolean;
  flow?: string;
  encryption?: string;
  tls?: boolean;
  ["skip-cert-verify"]?: boolean;
  servername?: string;
  ["reality-opts"]?: {
    ["public-key"]?: string;
    ["short-id"]?: string;
  } | null;
  ["client-fingerprint"]?: string;
  network?: string;
  raw?: Record<string, unknown>;
};

export type SubscriptionRecord = {
  id: string;
  name: string;
  url: string;
  ua: string;
  start_port: number;
  manual: boolean;
  content: string;
  nodes: ProxyNode[];
  selected_node_indices: number[];
  port_assignments: Record<string, number>;
  node_remarks: Record<string, string>;
};

export type NodeBinding = {
  sub_id: string;
  fingerprint: string;
  node_name: string;
};

export type PortSlot = {
  id: string;
  name: string;
  note: string;
  local_port: number;
  enabled: boolean;
  binding: NodeBinding | null;
};

export type AppSettings = {
  schema_version: number;
  subscriptions: SubscriptionRecord[];
  port_slots: PortSlot[];
  slots_migrated: boolean;
  subconverter: string;
  local_proxy_enabled: boolean;
  local_proxy_url: string;
  mihomo_path: string;
};

export type SlotBindingState = "unbound" | "valid" | "invalid";

export type PortSlotBindingView = {
  sub_id: string;
  sub_name: string;
  node_index: number | null;
  node_name: string;
  node_type: string;
  server: string;
  server_port: number;
};

export type PortSlotView = {
  id: string;
  name: string;
  note: string;
  local_port: number;
  enabled: boolean;
  state: SlotBindingState;
  invalid_reason: string | null;
  binding: PortSlotBindingView | null;
};

export type MigrationReport = {
  migrated: boolean;
  created_slots: number;
  messages: string[];
};

export type PortSlotBindingInput = {
  sub_id: string;
  node_index: number;
};

export type PortSlotBatchBindingInput = {
  slot_id: string;
  sub_id: string;
  node_index: number;
};

export type PortSlotInput = {
  name: string;
  local_port: number;
  note: string;
  enabled: boolean;
  binding: PortSlotBindingInput | null;
};

export type PortValidation = {
  status: "ok" | "invalid" | "conflict" | "occupied";
  message: string;
};

export type PortTrafficEntry = {
  local_port: number;
  upload: number;
  download: number;
  upload_speed: number;
  download_speed: number;
  connections: number;
};

export type PortConnection = {
  id: string;
  local_port: number;
  host: string;
  destination: string;
  rule: string;
  chain: string;
  process: string;
  network: string;
  upload: number;
  download: number;
  upload_speed: number;
  download_speed: number;
};

export type PortTrafficReport = {
  running: boolean;
  sampled_at: string;
  message: string;
  ports: PortTrafficEntry[];
  connections: PortConnection[];
};

export type RuntimeSnapshot = {
  config_path: string;
  mihomo_path: string;
  mihomo_exists: boolean;
  runtime_dir: string;
  running: boolean;
};

export type BootstrapPayload = {
  settings: AppSettings;
  slots: PortSlotView[];
  runtime: RuntimeSnapshot;
  migration?: MigrationReport | null;
};

export type UpsertSubscriptionInput = {
  name: string;
  url?: string;
  manual: boolean;
  content?: string;
};

export type LogEntry = {
  level: string;
  message: string;
  timestamp: string;
};

export type LatencyResult = {
  sub_id: string;
  node_index: number;
  result: string;
};

export type NodeTrafficConnection = {
  id: string;
  host: string;
  destination: string;
  rule: string;
  chains: string[];
  upload: number;
  download: number;
  upload_speed: number;
  download_speed: number;
  start: string;
  process: string;
  network: string;
  type: string;
};

export type NodeTrafficSnapshot = {
  sub_id: string;
  sub_name: string;
  node_index: number;
  node_name: string;
  local_port: number | null;
  running: boolean;
  sampled_at: string;
  upload_total: number;
  download_total: number;
  upload_speed: number;
  download_speed: number;
  connections: NodeTrafficConnection[];
  message: string;
};

export type NodeTrafficHistoryEntry = {
  id: string;
  time: string;
  host: string;
  destination: string;
  rule: string;
  chain: string;
  process: string;
  network: string;
  upload: number;
  download: number;
  upload_speed: number;
  download_speed: number;
  note: string;
};

export type NodeTrafficPanel = {
  snapshot: NodeTrafficSnapshot;
  session_upload: number;
  session_download: number;
  total_records: number;
  history: NodeTrafficHistoryEntry[];
};
