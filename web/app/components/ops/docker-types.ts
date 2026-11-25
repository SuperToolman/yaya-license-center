export type ContainerAction =
  "start" | "stop" | "restart" | "kill" | "pause" | "unpause";
export const actionLabels: Record<ContainerAction, string> = {
  start: "启动",
  stop: "停止",
  restart: "重启",
  kill: "强制停止",
  pause: "暂停",
  unpause: "恢复",
};
export const stateLabels: Record<string, string> = {
  running: "已启动",
  exited: "已停止",
  created: "已创建",
  paused: "已暂停",
  restarting: "重启中",
  removing: "删除中",
  dead: "异常",
};
export type DockerContainer = {
  association?: {
    instanceId: string;
    instanceName: string;
    customerId: string;
    customerName: string;
    active: boolean;
  } | null;
  health?: string | null;
  exitCode?: number | null;
  startedAt?: string;
  finishedAt?: string;
  mounts?: Array<{
    type: string;
    name: string | null;
    destination: string;
    rw: boolean;
  }>;
  id: string;
  name: string;
  image: string;
  state: string;
  createdAtMs: number | null;
  project: string | null;
  service: string | null;
  protected: boolean;
  terminalAllowed: boolean;
  allowedActions: ContainerAction[];
  cpuPercent: number | null;
  memoryPercent: number | null;
  memoryUsage: string | null;
  networkIO: string | null;
  addresses: Array<{ network: string; address: string }>;
  ports: Array<{
    containerPort: string;
    hostIp: string | null;
    hostPort: string | null;
  }>;
};
export type DockerContainers = {
  items: DockerContainer[];
  total: number;
  limit: number;
  truncated: boolean;
  statsError: string | null;
  capturedAtMs: number;
};
export type DockerInventory = {
  info: {
    serverVersion: string;
    operatingSystem: string;
    architecture: string;
    cpus: number;
    memory: number;
  };
  images: Array<{
    id: string;
    repository: string;
    tag: string;
    size: string;
    created: string;
  }>;
  networks: Array<{ id: string; name: string; driver: string; scope: string }>;
  volumes: Array<{ name: string; driver: string; scope: string }>;
  disk: Array<{
    Type: string;
    Size: string;
    Reclaimable: string;
    TotalCount: string;
  }> | null;
  diskError: string | null;
  capturedAtMs: number;
};
export function portUrl(
  port: DockerContainer["ports"][number],
  hostname: string,
) {
  if (
    !port.hostPort ||
    !port.containerPort.endsWith("/tcp") ||
    !/^\d+$/.test(port.hostPort)
  )
    return null;
  const raw = port.hostIp;
  const host = !raw || raw === "0.0.0.0" || raw === "::" ? hostname : raw;
  const wrapped =
    host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  return `${port.containerPort.startsWith("443/") ? "https" : "http"}://${wrapped}:${port.hostPort}`;
}
