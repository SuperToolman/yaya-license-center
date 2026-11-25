export type OpsCapability = { available: boolean; reason: string | null };
export type OpsContext = {
  environmentSupported: boolean;
  enabled: boolean;
  allowed: boolean;
  actions: string[];
  writesEnabled: boolean;
  hostTerminalEnabled: boolean;
  reason: string | null;
};
export type OpsNode = {
  nodeId: string;
  name: string;
  bootId: string;
  agentVersion: string;
  protocolVersion: number;
  docker: OpsCapability;
  hostMetrics: OpsCapability;
  nvidia: OpsCapability;
  status: "online" | "offline";
  lastSeenAtMs: number;
};
export type OpsNodes = {
  items: OpsNode[];
  connectionError: string | null;
  connectionErrorCode: string | null;
};
export type OpsContainer = {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  createdAtMs: number;
};
export type OpsContainers = {
  items: OpsContainer[];
  limit: number;
  truncated: boolean;
  capturedAtMs: number;
};
export type OpsGpu = {
  uuid: string;
  name: string;
  vendor: string;
  utilizationPercent: number | null;
  memoryUsedBytes: number | null;
  memoryTotalBytes: number | null;
  temperatureCelsius: number | null;
  powerWatts: number | null;
};
export type OpsMetrics = {
  nodeId: string;
  bootId: string;
  sequence: number;
  capturedAtMs: number;
  host: {
    cpuPercent: number | null;
    logicalCpus: number;
    memoryUsedBytes: number;
    memoryTotalBytes: number;
    uptimeSeconds: number;
  } | null;
  hostCapability: OpsCapability;
  gpuCapability: OpsCapability;
  gpus: OpsGpu[];
};
