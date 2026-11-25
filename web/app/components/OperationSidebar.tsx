"use client";
import { CreditCard, FaceRobot, FileText, Gear } from "@gravity-ui/icons";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useRef } from "react";
import { useOperation } from "./OperationProvider";

const STORAGE_KEY = "yaya-operation-sidebar-expanded";

const groups = [
  [
    "业务运营",
    [
      ["/orders", "订单中心", FileText],
      ["/customers", "客户管理", FaceRobot],
      ["/finance", "财务管理", CreditCard],
      ["/providers", "服务商", FaceRobot],
      ["/commissions", "提成与返现", CreditCard],
    ],
  ],
  [
    "平台管理",
    [
      ["/users", "用户管理", FaceRobot],
      ["/application-products", "应用管理", FileText],
      ["/platform-versions", "平台版本管理", FileText],
      ["/ai-employees", "AI员工", FaceRobot],
      ["/skills", "Skills", Gear],
    ],
  ],
] as const;
const opsGroups = [
  [
    "系统",
    [
      ["/ops/overview", "运维总览", Gear],
      ["/ops/nodes", "主机节点", FaceRobot],
      ["/ops/gpu", "GPU 监控", CreditCard],
      ["/ops/files", "文件", FileText],
    ],
  ],
] as const;
export default function OperationSidebar() {
  const pathname = usePathname();
  const { currentUser } = useOperation();
  const canAccessOps = currentUser?.role === "platform_admin";
  const visibleGroups = canAccessOps ? [...groups, ...opsGroups] : groups;
  const navRef = useRef<HTMLElement>(null);

  useEffect(() => {
    try {
      const saved: unknown = JSON.parse(
        localStorage.getItem(STORAGE_KEY) ?? "{}",
      );
      if (!saved || typeof saved !== "object" || Array.isArray(saved)) return;
      navRef.current?.querySelectorAll("details").forEach((group) => {
        group.open =
          (saved as Record<string, unknown>)[group.dataset.group!] === true;
      });
    } catch {
      // Keep the default collapsed state if browser storage is unavailable or invalid.
    }
  }, [canAccessOps]);

  function saveExpandedGroups() {
    let expanded: Record<string, boolean> = {};
    try {
      const saved: unknown = JSON.parse(
        localStorage.getItem(STORAGE_KEY) ?? "{}",
      );
      if (saved && typeof saved === "object" && !Array.isArray(saved))
        expanded = Object.fromEntries(
          Object.entries(saved).filter(
            (entry) => typeof entry[1] === "boolean",
          ),
        );
    } catch {
      /* Storage is optional. */
    }
    navRef.current?.querySelectorAll("details").forEach((group) => {
      expanded[group.dataset.group!] = group.open;
    });
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(expanded));
    } catch {
      // Navigation remains usable when browser storage is unavailable or full.
    }
  }

  return (
    <aside className="min-[761px]:sticky min-[761px]:top-0 flex min-[761px]:h-screen flex-col bg-background-inverse px-3 py-5 text-white">
      <div className="flex items-center gap-3 px-2 pb-6">
        <span className="grid h-9 w-9 place-items-center rounded-md bg-accent font-bold">
          Y
        </span>
        <span>
          <strong className="block text-sm">Yaya</strong>
          <small className="text-xs text-white/70">运营管理平台</small>
        </span>
      </div>
      <nav
        ref={navRef}
        aria-label="主导航"
        className="sidebar-navigation flex min-h-0 flex-col gap-1 overflow-y-auto"
      >
        {visibleGroups.map(([label, items]) => (
          <details
            key={label}
            data-group={items[0][0]}
            onToggle={saveExpandedGroups}
            className="sidebar-group group shrink-0"
          >
            <summary
              className={`flex min-h-10 cursor-pointer list-none items-center justify-between gap-3 rounded-md px-3 text-sm font-semibold transition-colors hover:bg-white/10 hover:text-white focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-white [&::-webkit-details-marker]:hidden ${items.some(([href]) => pathname === href || pathname.startsWith(`${href}/`)) ? "bg-white/10 text-white" : "text-white/70"}`}
            >
              {label}
              <svg
                aria-hidden="true"
                viewBox="0 0 16 16"
                fill="none"
                className="h-4 w-4 shrink-0 transition-transform duration-200 ease-out group-open:rotate-90 motion-reduce:transition-none"
              >
                <path
                  d="m6 4 4 4-4 4"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </summary>
            <div className="ml-3 mt-1 flex flex-col gap-1 border-l border-white/15 pl-2">
              {items.map(([href, title, Icon]) => (
                <Link
                  key={href}
                  href={href}
                  aria-current={
                    pathname === href || pathname.startsWith(`${href}/`)
                      ? "page"
                      : undefined
                  }
                  className={`flex min-h-10 shrink-0 items-center gap-2.5 rounded-md px-3 text-sm transition-colors ${pathname === href || pathname.startsWith(`${href}/`) ? "bg-accent text-white" : "text-white/70 hover:bg-white/10 hover:text-white"}`}
                >
                  <Icon className="h-4 w-4" />
                  {title}
                </Link>
              ))}
            </div>
          </details>
        ))}
        <Link
          href="/logs"
          aria-current={
            pathname === "/logs" || pathname.startsWith("/logs/")
              ? "page"
              : undefined
          }
          className={`flex min-h-10 shrink-0 items-center gap-2.5 rounded-md px-3 text-sm font-semibold transition-colors ${pathname === "/logs" || pathname.startsWith("/logs/") ? "bg-accent text-white" : "text-white/70 hover:bg-white/10 hover:text-white"}`}
        >
          <FileText className="h-4 w-4" />
          日志系统
        </Link>
        {canAccessOps && (
          <Link
            href="/ops/containers"
            aria-current={
              pathname === "/ops/containers" ||
              pathname.startsWith("/ops/containers/")
                ? "page"
                : undefined
            }
            className={`flex min-h-10 shrink-0 items-center gap-2.5 rounded-md px-3 text-sm font-semibold transition-colors ${pathname === "/ops/containers" || pathname.startsWith("/ops/containers/") ? "bg-accent text-white" : "text-white/70 hover:bg-white/10 hover:text-white"}`}
          >
            <FileText className="h-4 w-4" />
            容器管理
          </Link>
        )}
      </nav>
    </aside>
  );
}
