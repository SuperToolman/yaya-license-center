"use client";

import { FormEvent, useState } from "react";
import { Button, Card, Input } from "@heroui/react";
import { useOperation } from "./OperationProvider";

export default function OperationAuthGate({ children }: { children: React.ReactNode }) {
  const { authenticated, login } = useOperation();
  const [username, setUsername] = useState(""); const [password, setPassword] = useState("");
  const [error, setError] = useState(""); const [submitting, setSubmitting] = useState(false);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setSubmitting(true); setError("");
    const message = await login(username, password); if (message) setError(message); else setPassword(""); setSubmitting(false);
  }
  if (authenticated === null) return <main className="grid min-h-screen place-items-center text-sm text-muted">正在检查访问权限</main>;
  if (authenticated) return <>{children}</>;
  return <main className="grid min-h-screen place-items-center p-5"><Card className="w-full max-w-md p-7"><p className="mb-2 text-[11px] font-bold text-accent">YAYA OPERATION CENTER</p><h1 className="text-2xl font-semibold">运营管理平台</h1><form className="mt-6 space-y-4" onSubmit={submit}><Input aria-label="账号" value={username} onChange={(event) => setUsername(event.currentTarget.value)} required autoFocus fullWidth /><Input aria-label="密码" type="password" value={password} onChange={(event) => setPassword(event.currentTarget.value)} required fullWidth />{error ? <p className="text-sm text-danger" role="alert">{error}</p> : null}<Button type="submit" fullWidth isPending={submitting}>进入运营管理平台</Button></form></Card></main>;
}
