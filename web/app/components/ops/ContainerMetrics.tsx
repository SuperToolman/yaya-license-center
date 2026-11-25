"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import { Alert, Button, Card, Checkbox, Label } from "@heroui/react";
import { useEffect, useState } from "react";
import { useOperationResource } from "../../hooks/useOperationQuery";
type Sample = {
  capturedAt: number;
  state: string;
  cpuPercent: number | null;
  memoryUsageBytes: number | null;
  memoryLimitBytes: number | null;
  memoryPercent: number | null;
  networkRxBytes: number | null;
  networkTxBytes: number | null;
  networkRxBytesPerSecond: number | null;
  networkTxBytesPerSecond: number | null;
  writableLayerBytes: number | null;
  rootFsBytes: number | null;
  diskCheckedAt: number | null;
  restartCount: number | null;
  error?: string | null;
};
type Metrics = {
  latest: Sample | null;
  history: Sample[];
  alerts: Array<{ code: string; message: string }>;
  intervalSeconds: number;
  retentionHours: number;
};
function bytes(v: number | null | undefined) {
  if (v == null) return "不可用";
  if (v < 1024) return `${v.toFixed(0)} B`;
  const n = Math.min(Math.floor(Math.log(v) / Math.log(1024)), 4);
  return `${(v / 1024 ** n).toFixed(2)} ${["B", "KiB", "MiB", "GiB", "TiB"][n]}`;
}
function percent(v: number | null | undefined) {
  return v == null ? "不可用" : `${v.toFixed(2)}%`;
}
function Trend({
  history,
  field,
  label,
  format,
}: {
  history: Sample[];
  field: keyof Sample;
  label: string;
  format: (n: number) => string;
}) {
  const values = history.map((s) =>
    typeof s[field] === "number" ? (s[field] as number) : null,
  );
  const nums = values.filter((v): v is number => v !== null);
  const max = Math.max(...nums, 1);
  const start = history[0]?.capturedAt ?? 0;
  const finish = history.at(-1)?.capturedAt ?? start;
  let path = "";
  let connected = false;
  history.forEach((s, i) => {
    const n = values[i];
    if (n === null) {
      connected = false;
      return;
    }
    const x =
      finish === start
        ? 10
        : 10 + ((s.capturedAt - start) / (finish - start)) * 580;
    const y = 90 - (n / max) * 75;
    path += `${connected ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)} `;
    connected = true;
  });
  return (
    <Card className="min-w-0">
      <Card.Content>
        <Card.Description className="flex justify-between">
          <span>{label}</span>
          <span>峰值 {nums.length ? format(max) : "不可用"}</span>
        </Card.Description>
        <svg
          viewBox="0 0 600 105"
          role="img"
          aria-label={`${label}趋势`}
          className="h-28 w-full"
        >
          <path d="M10,90 L590,90" stroke="currentColor" opacity="0.2" />
          {nums.length ? (
            <path
              d={path}
              fill="none"
              stroke="var(--color-accent, #0284c7)"
              strokeWidth="2"
            />
          ) : (
            <text x="240" y="55" fill="currentColor" fontSize="14">
              暂无采样
            </text>
          )}
        </svg>
        <p className="flex justify-between text-[10px] text-muted">
          <span>
            {start ? new Date(start).toLocaleTimeString("zh-CN") : "—"}
          </span>
          <span>
            {finish ? new Date(finish).toLocaleTimeString("zh-CN") : "—"}
          </span>
        </p>
      </Card.Content>
    </Card>
  );
}
export default function ContainerMetrics({ id }: { id: string }) {
  const [revision, setRevision] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const [auto, setAuto] = useState(true);
  const query = useOperationResource<Metrics>(
    `/api/ops/local/containers/${id}/metrics`,
    revision,
  );
  useEffect(() => {
    if (!auto) return;
    const timer = setInterval(() => {
      if (!document.hidden) {
        setRevision((v) => v + 1);
        setNow(Date.now());
      }
    }, 5000);
    return () => clearInterval(timer);
  }, [auto]);
  useErrorNotification(query.error || query.data?.latest?.error, query.loading);
  const latest = query.data?.latest;
  const history = query.data?.history ?? [];
  const stale = latest && now - latest.capturedAt > 30000;
  return (
    <section aria-label="实例资源监控" className="space-y-3">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <Button
          size="sm"
          variant="secondary"
          isDisabled={query.loading}
          onPress={() => setRevision((v) => v + 1)}
        >
          刷新资源指标
        </Button>
        <Checkbox isSelected={auto} onChange={setAuto}>
          <Checkbox.Content>
            <Checkbox.Control>
              <Checkbox.Indicator />
            </Checkbox.Control>
            <Label>每 5 秒刷新指标</Label>
          </Checkbox.Content>
        </Checkbox>
      </div>
      {stale ? (
        <p role="status" className="mb-3 text-sm text-warning">
          采样已超过 30 秒未更新，以下为历史数据，请检查采集服务和 Docker。
        </p>
      ) : null}
      {query.data?.alerts.map((a) => (
        <Alert key={a.code} status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Description>{a.message}</Alert.Description>
          </Alert.Content>
        </Alert>
      ))}
      {!latest ? (
        <p className="text-sm text-muted">
          暂无采样。平台实例绑定的容器会自动采集，Docker
          可用后稍候刷新；未关联实例的容器暂不保存趋势。
        </p>
      ) : (
        <>
          <div className="my-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {[
              ["CPU 使用率", percent(latest.cpuPercent)],
              [
                "内存",
                `${bytes(latest.memoryUsageBytes)} / ${bytes(latest.memoryLimitBytes)}（${percent(latest.memoryPercent)}）`,
              ],
              [
                "网络接收 / 发送速率",
                `${latest.networkRxBytesPerSecond == null ? "不可用" : bytes(latest.networkRxBytesPerSecond) + "/s"} / ${latest.networkTxBytesPerSecond == null ? "不可用" : bytes(latest.networkTxBytesPerSecond) + "/s"}`,
              ],
              [
                "网络累计接收 / 发送",
                `${bytes(latest.networkRxBytes)} / ${bytes(latest.networkTxBytes)}`,
              ],
              ["容器可写层", bytes(latest.writableLayerBytes)],
              ["容器文件系统（含镜像）", bytes(latest.rootFsBytes)],
            ].map(([label, value]) => (
              <Card key={label}>
                <Card.Header>
                  <Card.Description>{label}</Card.Description>
                </Card.Header>
                <Card.Content>{value}</Card.Content>
              </Card>
            ))}
          </div>
          <p className="mb-3 text-xs text-muted">
            采样：{new Date(latest.capturedAt).toLocaleString("zh-CN")} ·
            重启计数 {latest.restartCount ?? "不可用"}。CPU 按单核 100%
            计；内存扣除可回收文件缓存。磁盘约每 60
            秒采样，不包含数据卷占用；Docker Desktop 指标属于容器环境。
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <Trend
              history={history}
              field="cpuPercent"
              label="CPU 使用率"
              format={percent}
            />
            <Trend
              history={history}
              field="memoryUsageBytes"
              label="内存使用量"
              format={bytes}
            />
            <Trend
              history={history}
              field="networkRxBytesPerSecond"
              label="网络接收速率"
              format={(n) => bytes(n) + "/s"}
            />
            <Trend
              history={history}
              field="networkTxBytesPerSecond"
              label="网络发送速率"
              format={(n) => bytes(n) + "/s"}
            />
          </div>
          <p className="mt-3 text-xs text-muted">
            展示最近 180 次采样，数据保留 24
            小时；缺失指标显示不可用，趋势保留缺口。宿主机和 GPU
            请到资源监控页面查看，支持情况以实际系统环境为准。
          </p>
        </>
      )}
    </section>
  );
}
