"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import {
  Button,
  Card,
  Checkbox,
  Input,
  Label,
  ListBox,
  Select,
  Tooltip,
} from "@heroui/react";
import { ArrowDownToLine, Magnifier, Play, Stop } from "@gravity-ui/icons";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  operationData,
  operationUrl,
  operationError,
} from "../../lib/operation-api";
import ContainerLogDatePicker from "./ContainerLogDatePicker";
import { getLocalTimeZone, parseDateTime } from "@internationalized/date";
import { socketUrl } from "./ContainerTerminal";
type Entry = { stream: string; timestamp: string | null; text: string };
type Snapshot = {
  entries?: Entry[];
  text: string;
  tty?: boolean;
  truncated?: boolean;
};
function dateValue(ms: number) {
  const d = new Date(ms);
  return new Date(ms - d.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 19);
}
export default function ContainerLogs({
  id,
  initialSince,
  fillAvailable = false,
}: {
  id: string;
  initialSince?: number;
  fillAvailable?: boolean;
}) {
  const [since, setSince] = useState(
    initialSince ? dateValue(initialSince) : "",
  );
  const [until, setUntil] = useState("");
  const [stream, setStream] = useState("all");
  const [keyword, setKeyword] = useState("");
  const [paused, setPaused] = useState(false);
  const [entries, setEntries] = useState<Entry[]>([]);
  const [error, setError] = useState("");
  useErrorNotification(error);
  const [status, setStatus] = useState("正在读取日志…");
  const [tty, setTty] = useState(false);
  const socket = useRef<WebSocket | null>(null);
  const logContainer = useRef<HTMLDivElement>(null);
  const live = useRef(true);
  const generation = useRef(0);
  const [receiving, setReceiving] = useState(false);
  const params = useCallback(() => {
    const query = new URLSearchParams({ tail: "200", stream });
    const start = since
      ? parseDateTime(since).toDate(getLocalTimeZone()).getTime()
      : null;
    const stop = until
      ? parseDateTime(until).toDate(getLocalTimeZone()).getTime()
      : null;
    if (
      (start !== null && !Number.isFinite(start)) ||
      (stop !== null && !Number.isFinite(stop)) ||
      (start !== null && stop !== null && start > stop)
    )
      throw Error("请输入有效的日志时间范围");
    if (start !== null) query.set("since", String(Math.floor(start / 1000)));
    if (stop !== null) query.set("until", String(Math.floor(stop / 1000)));
    return query;
  }, [since, until, stream]);
  const close = useCallback(() => {
    generation.current += 1;
    setReceiving(false);
    const ws = socket.current;
    socket.current = null;
    ws?.close();
  }, []);
  const load = useCallback(async () => {
    close();
    const request = generation.current;
    setError("");
    setStatus("正在读取日志…");
    try {
      const value = await operationData<Snapshot>(
        `/api/ops/local/containers/${id}/logs?${params()}`,
      );
      if (!live.current || generation.current !== request) return;
      setEntries(
        value.entries ?? [
          {
            stream: "combined",
            timestamp: null,
            text: value.text || "暂无日志",
          },
        ],
      );
      setTty(!!value.tty);
      setStatus(
        value.truncated
          ? "日志已截断为 4 MiB，请缩小时间范围"
          : "历史日志 · 最近 200 行",
      );
    } catch (e) {
      if (live.current && generation.current === request) {
        setError(operationError(e));
        setStatus("读取失败");
      }
    }
    return request;
  }, [id, params, close]);
  useEffect(() => {
    live.current = true;
    void Promise.resolve().then(() => {
      if (live.current) {
        void load().then((request) => {
          if (live.current && request === generation.current) follow();
        });
      }
    });
    return () => {
      live.current = false;
      close();
    };
  }, [id, close]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!paused && logContainer.current)
      logContainer.current.scrollTop = logContainer.current.scrollHeight;
  }, [entries, paused]);
  function follow() {
    close();
    setError("");
    try {
      const query = params();
      query.delete("until");
      setUntil("");
      setReceiving(true);
      setStatus("实时日志连接中…");
      const ws = new WebSocket(
        socketUrl(`/api/ops/local/containers/${id}/logs/live?${query}`),
      );
      socket.current = ws;
      ws.onmessage = (e) => {
        if (socket.current !== ws) return;
        try {
          const v = JSON.parse(e.data);
          if (v.type === "connected") {
            setTty(!!v.tty);
            setStatus("实时日志已连接");
          }
          if (v.type === "entry")
            setEntries((items) => {
              const entry: Entry = v.entry;
              if (
                entry.timestamp &&
                items.some(
                  (item) =>
                    item.timestamp === entry.timestamp &&
                    item.stream === entry.stream &&
                    item.text === entry.text,
                )
              )
                return items;
              return [...items, entry].slice(-2000);
            });
          if (v.type === "closed") {
            setReceiving(false);
            setStatus(v.reason || "实时日志已结束");
            socket.current = null;
            ws.close();
          }
        } catch {
          setReceiving(false);
          setError("日志消息解析失败");
          ws.close();
        }
      };
      ws.onerror = () => {
        if (socket.current === ws) {
          setReceiving(false);
          setError(
            "实时日志无法连接，请检查 Docker、登录状态或 WebSocket 代理配置",
          );
          setStatus("连接失败");
        }
      };
      ws.onclose = () => {
        if (socket.current === ws) {
          socket.current = null;
          setReceiving(false);
          setStatus((s) =>
            s === "实时日志已连接" || s === "实时日志连接中…"
              ? "实时日志已断开，请点击实时日志重新连接"
              : s,
          );
        }
      };
    } catch (e) {
      setReceiving(false);
      setError(operationError(e));
    }
  }
  async function download() {
    try {
      const query = params();
      query.set("download", "true");
      const response = await fetch(
        operationUrl(`/api/ops/local/containers/${id}/logs?${query}`),
        { credentials: "include" },
      );
      if (!response.ok)
        throw Error("日志下载失败，请检查时间范围和 Docker 状态");
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `container-${id.slice(0, 12)}.log`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      setError(operationError(e));
    }
  }
  const filtered = entries.filter(
    (e) =>
      (stream === "all" || e.stream === stream || e.stream === "combined") &&
      (!keyword || e.text.toLowerCase().includes(keyword.toLowerCase())),
  );
  return (
    <section
      aria-label="容器日志"
      className={`flex min-h-0 flex-col gap-2 ${fillAvailable ? "h-full" : "h-[min(640px,70dvh)]"}`}
    >
      <div className="grid shrink-0 gap-2 sm:grid-cols-3">
        <ContainerLogDatePicker
          label="日志开始时间"
          value={since}
          onChange={setSince}
        />
        <ContainerLogDatePicker
          label="日志结束时间"
          value={until}
          onChange={setUntil}
        />
        <Input
          aria-label="日志关键词"
          placeholder="搜索当前已接收日志"
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
        />
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-2">
        <Select
          aria-label="日志输出类型"
          selectedKey={stream}
          onSelectionChange={(key) => setStream(String(key))}
          className="w-32"
        >
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              <ListBox.Item id="all" textValue="全部输出">
                全部输出
              </ListBox.Item>
              <ListBox.Item id="stdout" textValue="标准输出">
                标准输出
              </ListBox.Item>
              <ListBox.Item id="stderr" textValue="错误输出">
                错误输出
              </ListBox.Item>
            </ListBox>
          </Select.Popover>
        </Select>
        <Tooltip>
          <Button
            isIconOnly
            size="sm"
            variant="secondary"
            aria-label="查询日志"
            onPress={() => void load()}
          >
            <Magnifier />
          </Button>
          <Tooltip.Content>查询历史日志</Tooltip.Content>
        </Tooltip>
        <Tooltip>
          <Button
            isIconOnly
            size="sm"
            variant={receiving ? "secondary" : "primary"}
            aria-label={receiving ? "停止接收" : "实时日志"}
            onPress={() => {
              if (receiving) {
                close();
                setStatus("已停止实时接收");
              } else follow();
            }}
          >
            {receiving ? <Stop /> : <Play />}
          </Button>
          <Tooltip.Content>
            {receiving ? "停止实时接收" : "开始实时日志"}
          </Tooltip.Content>
        </Tooltip>
        <Tooltip>
          <Button
            isIconOnly
            size="sm"
            variant="secondary"
            aria-label="下载日志"
            onPress={() => void download()}
          >
            <ArrowDownToLine />
          </Button>
          <Tooltip.Content>下载日志</Tooltip.Content>
        </Tooltip>
        <span role="status" className="min-w-0 flex-1 truncate text-xs">
          {status} · {filtered.length}/{entries.length} 条
        </span>
        <Checkbox isSelected={paused} onChange={setPaused}>
          <Checkbox.Content>
            <Checkbox.Control>
              <Checkbox.Indicator />
            </Checkbox.Control>
            <Label>暂停滚动</Label>
          </Checkbox.Content>
        </Checkbox>
      </div>
      {tty ? (
        <p className="shrink-0 text-xs text-muted">
          该容器启用了 TTY，Docker 已合并标准输出和错误输出，无法还原区分。
        </p>
      ) : null}
      <Card
        variant="secondary"
        ref={logContainer}
        className="min-h-0 flex-1 gap-0 overflow-auto font-mono text-xs"
        style={{
          backgroundColor: "#111827",
          color: "#e5e7eb",
          lineHeight: 1.6,
        }}
        role="log"
        aria-live="off"
        tabIndex={0}
        aria-label="日志内容"
      >
        {filtered.map((e, i) => (
          <div
            key={i}
            className="shrink-0 whitespace-pre-wrap break-all"
            style={e.stream === "stderr" ? { color: "#fda4af" } : undefined}
          >
            <span style={{ color: "#94a3b8" }}>
              {e.timestamp ?? ""} [{e.stream}]{" "}
            </span>
            {e.text}
          </div>
        ))}
        {!filtered.length && !error ? <p>暂无匹配日志</p> : null}
      </Card>
    </section>
  );
}
