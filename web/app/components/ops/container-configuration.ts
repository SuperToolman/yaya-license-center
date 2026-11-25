export type ContainerPort = {
  id: string;
  hostPort: string;
  containerPort: string;
  protocol: string;
  label?: string;
  managed?: boolean;
};

export type ContainerVolume = {
  id: string;
  name: string;
  destination: string;
  readOnly: boolean;
  managed?: boolean;
};

export type EnvironmentRow = {
  id: string;
  name: string;
  value: string;
  originalName?: string;
  originalValue?: string;
  isReadOnly?: boolean;
  description?: string;
};

export function serializeContainerEnvironment(
  rows: EnvironmentRow[],
): string[] {
  const names = new Set<string>();
  const result: string[] = [];
  for (const row of rows) {
    const name = row.name.trim();
    if (!name && !row.value) continue;
    if (!/^[a-zA-Z_][a-zA-Z0-9_]{0,127}$/.test(name))
      throw new Error(
        "环境变量名须以字母或下划线开头，仅包含字母、数字和下划线，最多 128 个字符。",
      );
    if (names.has(name)) throw new Error(`环境变量名称重复：${name}`);
    if (/[\0\r\n]/.test(row.value))
      throw new Error(`${name} 的值不能包含换行或空字符。`);
    names.add(name);
    result.push(`${name}=${row.value}`);
  }
  if (result.length > 128) throw new Error("环境变量最多 128 项。");
  if (new TextEncoder().encode(result.join("\n")).length > 32768)
    throw new Error("环境变量不得超过 32 KiB。");
  return result;
}
