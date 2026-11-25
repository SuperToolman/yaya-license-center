import type { EnvironmentRow } from "../../../../components/ops/container-configuration";
import { platformEnvironmentDescriptions } from "./environment-descriptions";

const instanceDefaults: Readonly<Record<string, string>> = {
  POSTGRES_DB: "yaya_low_code",
  POSTGRES_USER: "yaya",
  VALKEY_CACHE_TTL_HOURS: "8",
};
const generatedSecrets = new Set([
  "POSTGRES_PASSWORD",
  "VALKEY_PASSWORD",
  "AUTH_TOKEN_SECRET",
  "BACKEND_INTERNAL_TOKEN",
  "YAYA_BYOM_ENCRYPTION_KEY",
  "YAYA_AGENT_V2_WORKER_SECRET",
  "YAYA_EXECUTION_SECRET",
]);
// Keep consistent with api/src/platform_delivery/environment.rs.
const managedNames = new Set([
  "APP_ENV",
  "APP_HOST",
  "APP_PORT",
  "DATABASE_URL",
  "VALKEY_URL",
  "BACKEND_API_BASE_URL",
  "YAYA_LICENSE_PUBLIC_KEY_PATH",
  "YAYA_AGENT_V2_API_URL",
  "YAYA_AGENT_V2_CORDIS_HOST",
  "YAYA_AGENT_V2_OFFICE_PATH",
  "YAYA_AGENT_V2_EXECUTION_URL",
  "YAYA_EXECUTION_ROOT",
  "YAYA_UPLOAD_DIR",
  "YAYA_SETTINGS_PATH",
  "YAYA_VALKEY_SETTINGS_PATH",
  "YAYA_IDENTITY_SETTINGS_PATH",
  "YAYA_NOTIFICATION_SETTINGS_PATH",
  "YAYA_LICENSE_SETTINGS_PATH",
  "YAYA_SKILLS_PATH",
  "CORS_ALLOWED_ORIGINS",
  "AUTH_COOKIE_SECURE",
  "YAYA_PLATFORM_ADMIN_MOBILE",
  "YAYA_PLATFORM_INSTANCE_ID",
  "LD_PRELOAD",
  "LD_LIBRARY_PATH",
  "NODE_OPTIONS",
  "PATH",
]);

export type { EnvironmentRow } from "../../../../components/ops/container-configuration";

export function initialEnvironmentRows(
  environment: string[] = [],
  savedKeys: string[] = [],
  declarations: Array<{
    name: string;
    required?: boolean;
    default?: string | null;
  }> = [],
): EnvironmentRow[] {
  const rows = environment.map((entry) => {
    const separator = entry.indexOf("=");
    const name = entry.slice(0, separator);
    const value = savedKeys.includes(name) ? "" : entry.slice(separator + 1);
    return {
      id: name,
      name,
      value,
      originalName: name,
      originalValue: value,
      isReadOnly: managedNames.has(name),
    };
  });
  for (const declaration of declarations) {
    const existing = rows.find((row) => row.name === declaration.name);
    const value = savedKeys.includes(declaration.name)
      ? ""
      : (declaration.default ?? existing?.value ?? "");
    if (existing) {
      existing.value = value;
      // Deployment defaults are also applied by the API; only send user edits.
      existing.originalValue = value;
    } else {
      rows.push({
        id: declaration.name,
        name: declaration.name,
        value,
        originalName: declaration.name,
        originalValue: value,
        isReadOnly: managedNames.has(declaration.name),
      });
    }
  }
  for (const name of savedKeys) {
    if (!rows.some((row) => row.name === name) && !managedNames.has(name))
      rows.push({
        id: name,
        name,
        value: "",
        originalName: name,
        originalValue: "",
        isReadOnly: false,
      });
  }
  return rows.map((row) => {
    let value = row.value;
    if (!savedKeys.includes(row.name)) {
      if (generatedSecrets.has(row.name)) value = "";
      else if (instanceDefaults[row.name] !== undefined)
        value = instanceDefaults[row.name];
    }
    const description = platformEnvironmentDescriptions[row.name];
    return {
      ...row,
      value,
      originalValue: value,
      isReadOnly:
        row.isReadOnly ||
        generatedSecrets.has(row.name) ||
        row.name in instanceDefaults,
      description: generatedSecrets.has(row.name)
        ? `${description ?? "实例认证密钥。"} 创建时由平台自动生成，编辑时保留原值。`
        : row.name in instanceDefaults
          ? `${description ?? "实例默认配置。"} 由平台自动配置。`
          : row.isReadOnly
            ? `${description ?? "镜像运行环境配置。"} 由平台管理。`
            : description,
    };
  });
}

export function serializeEnvironment(
  rows: EnvironmentRow[],
): string | undefined {
  const names = new Set<string>();
  const lines: string[] = [];
  for (const row of rows) {
    const name = row.name.trim();
    if (!name && !row.value) continue;
    if (row.isReadOnly) continue;
    if (!/^[A-Z][A-Z0-9_]{0,79}$/.test(name))
      throw new Error(
        "环境变量名须以大写字母开头，仅包含大写字母、数字和下划线，最多 80 个字符。",
      );
    if (managedNames.has(name))
      throw new Error(`${name} 由平台管理，不能手动覆盖。`);
    if (names.has(name)) throw new Error(`环境变量名称重复：${name}`);
    names.add(name);
    if (/[\0\r\n]/.test(row.value))
      throw new Error(`${name} 的值不能包含换行或空字符。`);
    // Blank values retain the saved value or the platform/runtime default.
    if (
      row.value !== "" &&
      (name !== row.originalName || row.value !== row.originalValue)
    )
      lines.push(`${name}=${row.value}`);
  }
  const text = lines.join("\n");
  if (new TextEncoder().encode(text).length > 32768)
    throw new Error("环境变量不得超过 32 KiB。");
  return text || undefined;
}
