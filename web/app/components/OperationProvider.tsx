"use client";

import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { operationRequest } from "../lib/operation-api";
import type { OperationUser } from "../lib/operation-types";

type OperationContextValue = {
  authenticated: boolean | null;
  currentUser: OperationUser | null;
  refreshToken: number;
  refresh: () => void;
  login: (username: string, password: string) => Promise<string | null>;
};
const OperationContext = createContext<OperationContextValue | null>(null);

export function OperationProvider({ children }: { children: React.ReactNode }) {
  const [authenticated, setAuthenticated] = useState<boolean | null>(null);
  const [currentUser, setCurrentUser] = useState<OperationUser | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const refresh = useCallback(() => setRefreshToken((value) => value + 1), []);
  useEffect(() => {
    void (async () => {
      try {
        const { response, payload } = await operationRequest<{ user?: OperationUser }>("/api/session");
        setCurrentUser(response.ok ? payload.data?.user ?? null : null);
        setAuthenticated(response.ok);
      } catch { setAuthenticated(false); }
    })();
  }, []);
  const login = useCallback(async (username: string, password: string) => {
    try {
      const { response, payload } = await operationRequest<{ user?: OperationUser }>("/api/session", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ username, password }) });
      if (!response.ok) return payload.message || "账号或密码错误";
      setCurrentUser(payload.data?.user ?? null); setAuthenticated(true); refresh(); return null;
    } catch { return "无法连接运营管理服务"; }
  }, [refresh]);
  return <OperationContext.Provider value={{ authenticated, currentUser, refreshToken, refresh, login }}>{children}</OperationContext.Provider>;
}

export function useOperation() {
  const value = useContext(OperationContext);
  if (!value) throw new Error("useOperation must be used within OperationProvider");
  return value;
}
