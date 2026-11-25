"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import {
  Accordion,
  Button,
  Card,
  Chip,
  Separator,
  Tooltip,
} from "@heroui/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { CircleInfo, Play, Stop } from "@gravity-ui/icons";
import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";
import {
  operationUrl,
  operationData,
  operationError,
} from "../../lib/operation-api";

export function socketUrl(path: string) {
  const url = new URL(operationUrl(path), window.location.origin);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}
type Session = {
  sessionId: string;
  actorName: string | null;
  actorId: string;
  createdAt: number;
  connectedAt: number | null;
  disconnectedAt: number | null;
  status: string;
  reason: string | null;
  cleanupResult?: string | null;
};
const labels: Record<string, string> = {
  connecting: "连接中",
  connected: "已连接",
  closed: "已断开",
  failed: "连接失败",
  interrupted: "服务重启中断",
};

export default function ContainerTerminal({ id }: { id: string }) {
  const host = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const socket = useRef<WebSocket | null>(null);
  const [ready, setReady] = useState(false);
  const [status, setStatus] = useState("closed");
  const [reason, setReason] = useState("");
  const [history, setHistory] = useState<Session[]>([]);
  const [historyError, setHistoryError] = useState("");
  useErrorNotification(historyError || (status === "failed" ? reason : ""));
  const loadHistory = useCallback(async () => {
    try {
      const v = await operationData<Session[]>(
        `/api/ops/local/containers/${id}/terminal-sessions`,
      );
      setHistory(v);
      setHistoryError("");
    } catch (e) {
      setHistoryError(operationError(e));
    }
  }, [id]);
  useEffect(() => {
    let disposed = false;
    let observer: ResizeObserver | undefined;
    let input: { dispose: () => void } | undefined;
    Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit")])
      .then(([{ Terminal }, { FitAddon }]) => {
        if (disposed || !host.current) return;
        const term = new Terminal({
          cursorBlink: true,
          fontSize: 13,
          fontFamily: "Consolas, monospace",
          scrollback: 2000,
          convertEol: false,
          theme: { background: "#111827", foreground: "#e5e7eb" },
        });
        const addon = new FitAddon();
        term.loadAddon(addon);
        term.open(host.current);
        terminal.current = term;
        fit.current = addon;
        const resize = () => {
          addon.fit();
          if (socket.current?.readyState === WebSocket.OPEN)
            socket.current.send(
              JSON.stringify({
                type: "resize",
                cols: Math.min(500, Math.max(20, term.cols)),
                rows: Math.min(200, Math.max(5, term.rows)),
              }),
            );
        };
        observer = new ResizeObserver(resize);
        observer.observe(host.current);
        resize();
        input = term.onData((data) => {
          if (socket.current?.readyState === WebSocket.OPEN)
            for (let offset = 0; offset < data.length; offset += 2000)
              socket.current.send(
                JSON.stringify({
                  type: "input",
                  data: data.slice(offset, offset + 2000),
                }),
              );
        });
        setReady(true);
      })
      .catch(() => {
        if (!disposed) {
          setStatus("failed");
          setReason("终端组件加载失败，请刷新重试");
        }
      });
    void Promise.resolve().then(() => {
      if (!disposed) void loadHistory();
    });
    return () => {
      disposed = true;
      const ws = socket.current;
      socket.current = null;
      if (ws?.readyState === WebSocket.OPEN)
        ws.send(JSON.stringify({ type: "close" }));
      ws?.close();
      observer?.disconnect();
      input?.dispose();
      terminal.current?.dispose();
      terminal.current = null;
      fit.current = null;
    };
  }, [id, loadHistory]);
  function connect() {
    if (!ready || socket.current) return;
    setStatus("connecting");
    setReason("");
    terminal.current?.reset();
    const ws = new WebSocket(
      socketUrl(`/api/ops/local/containers/${id}/terminal`),
    );
    ws.binaryType = "arraybuffer";
    socket.current = ws;
    ws.onmessage = (event) => {
      if (socket.current !== ws) return;
      if (event.data instanceof ArrayBuffer) {
        terminal.current?.write(new Uint8Array(event.data));
        return;
      }
      try {
        const v = JSON.parse(event.data);
        if (v.type === "connected") {
          setStatus("connected");
          fit.current?.fit();
          ws.send(
            JSON.stringify({
              type: "resize",
              cols: Math.min(500, Math.max(20, terminal.current?.cols ?? 80)),
              rows: Math.min(200, Math.max(5, terminal.current?.rows ?? 24)),
            }),
          );
          terminal.current?.focus();
          void loadHistory();
        }
        if (v.type === "closed") {
          setReason(v.reason || "连接已断开");
          setStatus("closed");
          socket.current = null;
          ws.close();
          void loadHistory();
        }
      } catch {
        setReason("终端返回无效消息");
        ws.close();
      }
    };
    ws.onerror = () => {
      if (socket.current === ws) {
        setStatus("failed");
        setReason(
          "终端连接失败，请确认 Docker、容器状态和管理员会话，并检查代理是否允许 WebSocket",
        );
      }
    };
    ws.onclose = () => {
      if (socket.current === ws) {
        socket.current = null;
        setStatus((s) => (s === "failed" ? s : "closed"));
        void loadHistory();
      }
    };
  }
  function disconnect() {
    const ws = socket.current;
    if (ws?.readyState === WebSocket.OPEN)
      ws.send(JSON.stringify({ type: "close" }));
    socket.current = null;
    ws?.close();
    setStatus("closed");
    setReason("用户断开连接");
    void loadHistory();
  }
  const connected = status === "connecting" || status === "connected";
  return (
    <section
      aria-label="容器交互终端"
      className="flex h-full min-h-0 flex-col gap-2"
    >
      <div className="flex shrink-0 items-center gap-2">
        <Tooltip>
          <Button
            isIconOnly
            size="sm"
            variant={connected ? "secondary" : "primary"}
            isDisabled={!ready}
            aria-label={connected ? "断开终端" : "连接终端"}
            onPress={connected ? disconnect : connect}
          >
            {connected ? <Stop /> : <Play />}
          </Button>
          <Tooltip.Content>
            {connected
              ? status === "connecting"
                ? "取消终端连接"
                : "断开终端"
              : "连接终端"}
          </Tooltip.Content>
        </Tooltip>
        <Chip
          role="status"
          size="sm"
          variant="soft"
          color={
            status === "connected"
              ? "success"
              : status === "failed"
                ? "danger"
                : status === "connecting"
                  ? "accent"
                  : "default"
          }
        >
          {labels[status]}
        </Chip>
        <Card.Description className="min-w-0 flex-1 truncate">
          {reason && status !== "failed"
            ? reason
            : "/bin/sh · 空闲 10 分钟断开 · 最长 1 小时"}
        </Card.Description>
        <Tooltip>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label="终端使用说明"
          >
            <CircleInfo />
          </Button>
          <Tooltip.Content>
            当前容器 /bin/sh，空闲 10 分钟自动断开，单次最长 1
            小时。切换标签或关闭窗口将断开连接。
          </Tooltip.Content>
        </Tooltip>
      </div>
      <Card
        ref={host}
        aria-label="终端输入输出"
        className="min-h-0 min-w-0 flex-1 overflow-hidden"
        style={{ backgroundColor: "#111827" }}
      >
        {null}
      </Card>
      <Accordion className="shrink-0">
        <Accordion.Item id="history">
          <Accordion.Heading>
            <Accordion.Trigger>
              终端连接记录（{history.length}）<Accordion.Indicator />
            </Accordion.Trigger>
          </Accordion.Heading>
          <Accordion.Panel>
            <Accordion.Body className="max-h-[25dvh] overflow-y-auto">
              {history.map((s) => (
                <div key={s.sessionId}>
                  <p>
                    {s.actorName || s.actorId} · {labels[s.status] || s.status}
                  </p>
                  <p>
                    连接：
                    {s.connectedAt
                      ? new Date(s.connectedAt).toLocaleString("zh-CN")
                      : "未连接"}{" "}
                    · 断开：
                    {s.disconnectedAt
                      ? new Date(s.disconnectedAt).toLocaleString("zh-CN")
                      : "—"}
                  </p>
                  {s.reason ? <p>{s.reason}</p> : null}
                  {s.cleanupResult ? <p>{s.cleanupResult}</p> : null}
                  <Separator />
                </div>
              ))}
              {!history.length && !historyError ? (
                <p className="text-sm text-muted">暂无连接记录</p>
              ) : null}
            </Accordion.Body>
          </Accordion.Panel>
        </Accordion.Item>
      </Accordion>
    </section>
  );
}
