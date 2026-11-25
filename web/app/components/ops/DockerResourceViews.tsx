import { DataPanel } from "../ui/DataPanel";
import { DataTable } from "../ui/DataTable";
import { Table } from "@heroui/react";
import type { DockerContainer, DockerInventory } from "./docker-types";
export function DockerResourceViews({
  tab,
  data,
  projects,
  all,
}: {
  tab: string;
  data: DockerInventory | null;
  projects: string[];
  all: DockerContainer[];
}) {
  return (
    <>
      {tab === "compose" ? (
        <DataPanel
          title="编排项目"
          note="读取 Docker Compose 项目标签，展示现有编排及容器；此处不编辑编排文件。"
        >
          <DataTable
            label="编排项目"
            columns={["项目名称", "容器数", "运行中", "服务"]}
            empty="当前列表没有 Docker Compose 编排项目"
          >
            {projects.map((project) => {
              const group = all.filter((i) => i.project === project);
              return (
                <Table.Row key={project} id={project}>
                  <Table.Cell>{project}</Table.Cell>
                  <Table.Cell>{group.length}</Table.Cell>
                  <Table.Cell>
                    {group.filter((i) => i.state === "running").length}
                  </Table.Cell>
                  <Table.Cell>
                    {[
                      ...new Set(group.map((i) => i.service).filter(Boolean)),
                    ].join("、") || "—"}
                  </Table.Cell>
                </Table.Row>
              );
            })}
          </DataTable>
        </DataPanel>
      ) : null}
      {tab === "images" ? (
        <DataPanel title="本地镜像" note="同一镜像可以拥有多个标签。">
          <DataTable
            label="本地镜像"
            columns={["镜像", "标签", "镜像 ID", "大小", "创建时间"]}
            empty="暂无本地镜像"
          >
            {data?.images.map((item, index) => (
              <Table.Row key={item.id + index} id={item.id + index}>
                <Table.Cell>{item.repository}</Table.Cell>
                <Table.Cell>{item.tag}</Table.Cell>
                <Table.Cell>
                  <code title={item.id}>
                    {item.id.replace("sha256:", "").slice(0, 12)}
                  </code>
                </Table.Cell>
                <Table.Cell>{item.size}</Table.Cell>
                <Table.Cell>{item.created}</Table.Cell>
              </Table.Row>
            ))}
          </DataTable>
        </DataPanel>
      ) : null}
      {tab === "networks" ? (
        <DataPanel title="网络" note="查看当前 Docker 网络及驱动。">
          <DataTable
            label="Docker 网络"
            columns={["网络名称", "驱动", "范围", "网络 ID"]}
            empty="暂无网络"
          >
            {data?.networks.map((item) => (
              <Table.Row key={item.id} id={item.id}>
                <Table.Cell>{item.name}</Table.Cell>
                <Table.Cell>{item.driver}</Table.Cell>
                <Table.Cell>{item.scope}</Table.Cell>
                <Table.Cell>
                  <code title={item.id}>{item.id.slice(0, 12)}</code>
                </Table.Cell>
              </Table.Row>
            ))}
          </DataTable>
        </DataPanel>
      ) : null}
      {tab === "volumes" ? (
        <DataPanel title="存储卷" note="此处仅查看，不删除客户数据。">
          <DataTable
            label="Docker 存储卷"
            columns={["卷名称", "驱动", "范围"]}
            empty="暂无存储卷"
          >
            {data?.volumes.map((item) => (
              <Table.Row key={item.name} id={item.name}>
                <Table.Cell>
                  <span className="break-all">{item.name}</span>
                </Table.Cell>
                <Table.Cell>{item.driver}</Table.Cell>
                <Table.Cell>{item.scope}</Table.Cell>
              </Table.Row>
            ))}
          </DataTable>
        </DataPanel>
      ) : null}
      {tab === "configuration" ? (
        <DataPanel
          title="Docker 环境"
          note="Docker Desktop 的资源配置属于其 Linux 虚拟机，不代表 Windows 物理主机的使用率。"
        >
          <dl className="grid gap-5 px-4 pb-5 sm:grid-cols-2">
            {[
              ["服务版本", data?.info.serverVersion],
              ["运行环境", data?.info.operatingSystem],
              ["架构", data?.info.architecture],
              ["可用 CPU", data?.info.cpus],
              [
                "可用内存",
                data
                  ? `${(data.info.memory / 1024 ** 3).toFixed(2)} GiB`
                  : null,
              ],
            ].map(([label, value]) => (
              <div key={label}>
                <dt className="text-xs text-muted">{label}</dt>
                <dd className="mt-2 text-sm">{value ?? "—"}</dd>
              </div>
            ))}
          </dl>
        </DataPanel>
      ) : null}
    </>
  );
}
