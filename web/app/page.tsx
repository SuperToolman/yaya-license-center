"use client";

import { FormEvent, useEffect, useState } from "react";

type License = { license: string; licenseId: string; subject: string; modules: string[]; issuedAt: number; expiresAt: number; moduleExpiresAt: Record<string, number>; platformStatus: LicenseStatus; moduleStatuses: Record<string, LicenseStatus> };
type Envelope<T> = { message: string; data: T | null };
type LicenseStatus = "unactivated" | "running" | "expired" | "destroyed";

const licenseModules = [
  { id: "platform", name: "低代码平台", description: "基础模块，默认包含", required: true },
  { id: "communication", name: "Communication", description: "通信后置模块", required: false },
];
const apiBaseUrl = process.env.NEXT_PUBLIC_LICENSE_API_BASE_URL
  ?? (typeof window === "undefined" ? "http://127.0.0.1:8779" : `${window.location.protocol}//${window.location.hostname}:8779`);

function formatDate(timestamp: number) {
  return new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(new Date(timestamp * 1000));
}

function defaultExpiryDate() {
  const date = new Date();
  date.setFullYear(date.getFullYear() + 1);
  return date.toISOString().slice(0, 10);
}

function defaultModuleExpiryDate() {
  const date = new Date();
  date.setDate(date.getDate() + 30);
  return date.toISOString().slice(0, 10);
}

const statusLabel: Record<LicenseStatus, string> = { unactivated: "未激活", running: "运行", expired: "过期", destroyed: "已销毁" };

function remainingDays(expiresAt: string) {
  const expiry = new Date(`${expiresAt}T23:59:59`).getTime();
  return Math.max(1, Math.ceil((expiry - Date.now()) / (24 * 60 * 60 * 1000)));
}

function expiryNotice(expiresAt: string) {
  return new Intl.DateTimeFormat("zh-CN", { year: "numeric", month: "long", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false })
    .format(new Date(`${expiresAt}T23:59:59`));
}

export default function LicenseCenterPage() {
  const [authenticated, setAuthenticated] = useState<boolean | null>(null);
  const [adminToken, setAdminToken] = useState("");
  const [licenses, setLicenses] = useState<License[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [subject, setSubject] = useState("");
  const [modules, setModules] = useState<string[]>(["platform"]);
  const [platformExpiresAt, setPlatformExpiresAt] = useState(defaultExpiryDate);
  const [moduleExpiresAt, setModuleExpiresAt] = useState<Record<string, string>>({});
  const [error, setError] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const validDays = remainingDays(platformExpiresAt);
  const expiryAt = expiryNotice(platformExpiresAt);

  async function request<T>(path: string, options?: RequestInit) {
    const response = await fetch(`${apiBaseUrl}${path}`, { ...options, credentials: "include" });
    const payload = await response.json() as Envelope<T>;
    return { response, payload };
  }

  async function loadLicenses() {
    setIsLoading(true);
    setError("");
    try {
      const { response, payload } = await request<License[]>("/api/licenses");
      if (response.status === 401) { setAuthenticated(false); return; }
      if (!response.ok || !payload.data) { setError(payload.message || "许可证列表读取失败"); return; }
      setLicenses(payload.data);
    } catch { setError("无法连接许可证服务"); }
    finally { setIsLoading(false); }
  }

  useEffect(() => {
    void (async () => {
      try {
        const { response } = await request<{ authenticated: boolean }>("/api/session");
        if (response.ok) { setAuthenticated(true); await loadLicenses(); } else { setAuthenticated(false); }
      } catch { setError("无法连接许可证服务"); setAuthenticated(false); }
    })();
  }, []);

  async function login(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setIsSubmitting(true);
    try {
      const { response, payload } = await request<{ authenticated: boolean }>("/api/session", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ adminToken }) });
      if (!response.ok) { setError(payload.message || "管理员令牌无效"); return; }
      setAdminToken("");
      setAuthenticated(true);
      await loadLicenses();
    } catch { setError("无法连接许可证服务"); }
    finally { setIsSubmitting(false); }
  }

  function toggleModule(module: string) {
    if (module === "platform") return;
    setModules((current) => current.includes(module) ? current.filter((item) => item !== module) : [...current, module]);
    setModuleExpiresAt((current) => current[module] ? current : { ...current, [module]: defaultModuleExpiryDate() });
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setIsSubmitting(true);
    try {
      const submittedModules = Array.from(new Set(["platform", ...modules]));
      const moduleExpiries = Object.fromEntries(submittedModules.filter((module) => module !== "platform").map((module) => [module, new Date(`${moduleExpiresAt[module] || platformExpiresAt}T23:59:59`).toISOString()]));
      const { response, payload } = await request<License>("/api/licenses", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ subject, modules: submittedModules, platformExpiresAt: new Date(`${platformExpiresAt}T23:59:59`).toISOString(), moduleExpiresAt: moduleExpiries }) });
      if (response.status === 401) { setAuthenticated(false); setShowForm(false); return; }
      if (!response.ok || !payload.data) { setError(payload.message || "签发失败"); return; }
      setShowForm(false); setSubject(""); setModules(["platform"]); setPlatformExpiresAt(defaultExpiryDate()); setModuleExpiresAt({});
      await loadLicenses();
    } catch { setError("无法连接许可证服务"); }
    finally { setIsSubmitting(false); }
  }

  async function destroy(license: License) {
    if (!window.confirm(`确认销毁许可证 ${license.licenseId} 吗？此操作不可恢复。`)) return;
    setError("");
    try {
      const { response, payload } = await request(`/api/licenses/${license.licenseId}/destroy`, { method: "POST" });
      if (!response.ok) throw new Error(payload.message || "销毁许可证失败");
      await loadLicenses();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "销毁许可证失败"); }
  }

  if (authenticated === null) return <main className="auth-page"><p className="muted">正在检查访问权限...</p></main>;
  if (!authenticated) return <main className="auth-page"><section className="token-prompt"><p className="eyebrow">LICENSE CENTER</p><h1>许可证中心</h1><p>请输入管理员令牌以访问许可证签发中心。</p><form onSubmit={login}><label>管理员令牌<input type="password" value={adminToken} onChange={(event) => setAdminToken(event.target.value)} required autoFocus /></label>{error ? <p className="error" role="alert">{error}</p> : null}<button className="primary" disabled={isSubmitting}>{isSubmitting ? "验证中..." : "进入许可证中心"}</button></form></section></main>;

  return <main className="app-shell"><aside className="sidebar"><div className="brand"><span className="brand-mark">Y</span><span>许可证中心</span></div><nav aria-label="主导航"><button className="nav-item active">许可证签发</button></nav></aside><section className="content"><header className="page-header"><div><p className="eyebrow">LICENSE MANAGEMENT</p><h1>许可证列表</h1><p>查看所有已签发许可证的完整授权信息。</p></div><div className="header-actions"><button className="icon-button" onClick={() => void loadLicenses()} title="刷新列表" aria-label="刷新列表">↻</button><button className="primary" onClick={() => { setError(""); setShowForm(true); }}>新增许可证</button></div></header><section className="list-panel" aria-live="polite"><div className="list-summary"><span>全部许可证</span><strong>{licenses.length} 张</strong></div>{isLoading ? <p className="empty">正在读取许可证...</p> : licenses.length === 0 ? <p className="empty">暂无已签发的许可证。</p> : <div className="license-list">{licenses.map((license) => <article className="license-item" key={license.licenseId}><dl><div><dt>许可证编号</dt><dd className="mono">{license.licenseId}</dd></div><div><dt>授权主体</dt><dd>{license.subject}</dd></div><div><dt>平台状态</dt><dd>{statusLabel[license.platformStatus]}</dd></div><div><dt>签发时间</dt><dd>{formatDate(license.issuedAt)}</dd></div><div><dt>平台有效至</dt><dd>{formatDate(license.expiresAt)}</dd></div><div><dt>模块状态</dt><dd>{license.modules.map((module) => <p key={module}>{module === "platform" ? "低代码平台" : module}：{statusLabel[license.moduleStatuses[module] ?? "unactivated"]}，有效至 {formatDate(license.moduleExpiresAt[module] ?? license.expiresAt)}</p>)}</dd></div></dl>{license.platformStatus !== "destroyed" ? <button className="secondary" onClick={() => void destroy(license)}>销毁许可证</button> : null}<details><summary>查看完整许可证</summary><textarea readOnly value={license.license} aria-label={`${license.licenseId} 的许可证内容`} /></details></article>)}</div>}</section>{!showForm && error ? <p className="error" role="alert">{error}</p> : null}</section>{showForm ? <div className="modal-backdrop" role="presentation"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="issue-title"><header><div><p className="eyebrow">NEW LICENSE</p><h2 id="issue-title">新增许可证</h2></div><button className="close" type="button" onClick={() => setShowForm(false)} aria-label="关闭">×</button></header><form onSubmit={submit}><label>授权主体<input value={subject} onChange={(event) => setSubject(event.currentTarget.value)} placeholder="客户或实例名称" required autoFocus /></label><fieldset><legend>模块与独立有效期</legend><div className="module-list">{licenseModules.map((module) => <div className="module-option" key={module.id}><label><input type="checkbox" checked={modules.includes(module.id)} disabled={module.required} onChange={() => toggleModule(module.id)} /><span><strong>{module.name}</strong><small>{module.description}</small></span></label>{modules.includes(module.id) ? <label>有效期<input type="date" min={new Date().toISOString().slice(0, 10)} value={module.id === "platform" ? platformExpiresAt : moduleExpiresAt[module.id] ?? platformExpiresAt} onChange={(event) => { const value = event.currentTarget.value; if (module.id === "platform") { setPlatformExpiresAt(value); } else { setModuleExpiresAt((current) => ({ ...current, [module.id]: value })); } }} required /></label> : null}</div>)}</div></fieldset><div className="expiry-fields"><label>平台有效天数<input type="number" value={validDays} disabled aria-label="根据当前时间和平台有效期计算的有效天数" /></label></div><p className="expiry-notice">平台将在 {expiryAt} 后过期。各可选模块可设置更短的试用期。</p>{error ? <p className="error" role="alert">{error}</p> : null}<footer><button type="button" className="secondary" onClick={() => setShowForm(false)}>取消</button><button className="primary" type="submit" disabled={isSubmitting}>{isSubmitting ? "签发中..." : "签发许可证"}</button></footer></form></section></div> : null}</main>;
}
