export type PlatformInstance = {
  instanceId: string;
  authorizationId?: string | null;
  licenseId?: string | null;
  appliedLicenseId?: string | null;
  licenseSyncStatus?: string;
  licenseSyncError?: string | null;
  licenseSyncedAt?: number | null;
  name: string;
  customerId: string;
  customerName: string;
  packageId: string;
  version: string;
  containerName: string;
  environmentKeys: string[];
  containerId: string | null;
  imageId: string | null;
  networkMode?: "bridge" | "macvlan";
  networkName?: string;
  webPort: number;
  apiPort: number;
  publicUrl: string;
  notes: string;
  status: "not_deployed" | "deploying" | "running" | "failed" | "archived";
  runtime?: {
    state: string;
    checkedAt: number;
    health: string | null;
    exitCode: number | null;
    message: string | null;
  } | null;
  createdAt: number;
  updatedAt: number;
};
export type InstanceOptions = {
  customers: Array<{ id: string; name: string }>;
  packages: Array<{
    id: string;
    name: string;
    environment?: string[] | null;
    deployment?: {
      webPort: number;
      apiPort: number;
      volumes: Array<{ key: string; destination: string }>;
      environment: Array<{
        name: string;
        required?: boolean;
        default?: string | null;
      }>;
    } | null;
  }>;
};
export const instanceStatusLabel: Record<PlatformInstance["status"], string> = {
  not_deployed: "待部署",
  deploying: "部署中",
  running: "运行中",
  failed: "部署失败",
  archived: "已归档",
};
export type DockerRuntime = {
  available: boolean;
  message: string;
  serverVersion: string | null;
};
export type Deployment = {
  deploymentId: string;
  status: string;
  stage: string;
  logTail: string;
  error: string | null;
  createdAt: number;
  finishedAt: number | null;
};

export function instanceRuntimeLabel(item: PlatformInstance) {
  if (
    !item.containerId ||
    item.status === "archived" ||
    item.status === "deploying"
  )
    return instanceStatusLabel[item.status];
  const labels: Record<string, string> = {
    running: "运行中",
    exited: "已停止",
    created: "已创建",
    paused: "已暂停",
    restarting: "重启中",
    dead: "容器异常",
    removing: "删除中",
    missing: "容器已删除",
    mismatch: "容器绑定异常",
    unknown: "状态未知",
  };
  return labels[item.runtime?.state ?? "unknown"] ?? "状态未知";
}

export type InstanceNetworks = {
  items: Array<{
    name: string;
    driver: "bridge" | "macvlan";
    subnets: string[];
    internal: boolean;
  }>;
  hostSupported: boolean;
  hostReason: string;
};
