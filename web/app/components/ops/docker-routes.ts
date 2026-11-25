export const dockerTabs = [
  ["overview", "概述"],
  ["containers", "容器"],
  ["compose", "编排"],
  ["images", "镜像"],
  ["networks", "网络"],
  ["volumes", "存储卷"],
  ["configuration", "配置"],
] as const;

export type DockerTab = (typeof dockerTabs)[number][0];

export function containerSearchHref(name: string) {
  return `/ops/containers/containers?${new URLSearchParams({ search: name })}`;
}
