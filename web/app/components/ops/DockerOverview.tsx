import { DataPanel } from "../ui/DataPanel";
import type { DockerInventory } from "./docker-types";
type ResourceTab = "compose" | "images" | "networks" | "volumes";
export function DockerOverview({
  data,
  count,
  ready,
  projects,
  chooseContainers,
  setTab,
}: {
  data: DockerInventory | null;
  count: { all: number; running: number; stopped: number; paused: number };
  ready: boolean;
  projects: string[];
  chooseContainers: (value: string) => void;
  setTab: (tab: ResourceTab) => void;
}) {
  return (
    <>
      <DataPanel
        title="容器"
        note="查看当前 Docker 容器状态，点击统计可筛选容器。"
      >
        <div className="grid grid-cols-2 gap-3 px-4 pb-5 md:grid-cols-4">
          {(
            [
              ["all", "所有"],
              ["running", "已启动"],
              ["stopped", "已停止"],
              ["paused", "已暂停"],
            ] as const
          ).map(([key, label]) => (
            <button
              key={key}
              onClick={() => chooseContainers(key)}
              className="rounded-xl border border-border p-4 text-left hover:bg-default"
            >
              <span className="text-sm text-muted">{label}</span>
              <strong className="mt-2 block text-3xl text-accent">
                {!ready ? "—" : count[key]}
              </strong>
            </button>
          ))}
        </div>
      </DataPanel>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        {(
          [
            ["compose", "编排", ready ? projects.length : "—"],
            [
              "images",
              "镜像",
              data ? new Set(data.images.map((i) => i.id)).size : "—",
            ],
            ["networks", "网络", data?.networks.length ?? "—"],
            ["volumes", "存储卷", data?.volumes.length ?? "—"],
          ] as const
        ).map(([key, label, value]) => (
          <button
            key={key}
            onClick={() => setTab(key)}
            className="mb-4 rounded-2xl border border-border bg-surface p-5 text-left hover:bg-default"
          >
            <span className="text-sm text-muted">{label}</span>
            <strong className="mt-3 block text-2xl text-accent">{value}</strong>
          </button>
        ))}
      </div>
      <DataPanel
        title="磁盘占用"
        note={
          data?.diskError ||
          "Docker 报告的已用空间与可回收空间；此处不执行清理。"
        }
      >
        <div className="grid gap-4 px-4 pb-5 md:grid-cols-4">
          {data?.disk?.map((item) => (
            <div key={item.Type}>
              <p className="text-sm font-medium">
                {(
                  {
                    Images: "镜像",
                    Containers: "容器",
                    "Local Volumes": "本地存储卷",
                    "Build Cache": "构建缓存",
                  } as Record<string, string>
                )[item.Type] ?? item.Type}
              </p>
              <p className="mt-2 text-sm">已用：{item.Size}</p>
              <p className="mt-1 text-xs text-muted">
                可回收：{item.Reclaimable}
              </p>
            </div>
          )) ?? <p className="text-sm text-muted">磁盘统计暂不可用</p>}
        </div>
      </DataPanel>
    </>
  );
}
