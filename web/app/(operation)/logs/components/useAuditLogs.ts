"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { operationData, operationError } from "../../../lib/operation-api";
import type { AuditLog } from "../../../lib/operation-types";

const PAGE_SIZE = 50;

export function useAuditLogs(filters: string, enabled: boolean) {
  const [revision, setRevision] = useState(0);
  const key = JSON.stringify([filters, revision, enabled]);
  const [state, setState] = useState<{
    key: string;
    items: AuditLog[];
    loading: boolean;
    error: string;
    hasMore: boolean;
  }>({ key: "", items: [], loading: false, error: "", hasMore: false });
  const active = useRef<{
    key: string;
    controller: AbortController;
    busy: boolean;
  } | null>(null);

  useEffect(() => {
    const request = { key, controller: new AbortController(), busy: true };
    active.current = request;
    if (!enabled) return () => request.controller.abort();
    void operationData<AuditLog[]>(`/api/logs?limit=${PAGE_SIZE}&${filters}`, {
      signal: request.controller.signal,
    })
      .then((items) => {
        if (!request.controller.signal.aborted)
          setState({
            key,
            items,
            loading: false,
            error: "",
            hasMore: items.length === PAGE_SIZE,
          });
      })
      .catch((cause) => {
        if (!request.controller.signal.aborted)
          setState({
            key,
            items: [],
            loading: false,
            error: operationError(cause),
            hasMore: false,
          });
      })
      .finally(() => {
        request.busy = false;
      });
    return () => request.controller.abort();
  }, [key, filters, enabled]);

  const loadMore = useCallback(() => {
    const request = active.current;
    if (
      !enabled ||
      !request ||
      request.key !== key ||
      request.busy ||
      state.key !== key ||
      !state.hasMore
    )
      return;
    const before = state.items.at(-1)?.id;
    if (before === undefined) return;
    request.busy = true;
    setState((current) => ({ ...current, loading: true, error: "" }));
    void operationData<AuditLog[]>(
      `/api/logs?limit=${PAGE_SIZE}&before=${before}&${filters}`,
      { signal: request.controller.signal },
    )
      .then((items) => {
        if (!request.controller.signal.aborted)
          setState((current) =>
            current.key !== key
              ? current
              : {
                  ...current,
                  items: [
                    ...current.items,
                    ...items.filter(
                      (item) =>
                        !current.items.some((old) => old.id === item.id),
                    ),
                  ],
                  loading: false,
                  hasMore: items.length === PAGE_SIZE,
                },
          );
      })
      .catch((cause) => {
        if (!request.controller.signal.aborted)
          setState((current) =>
            current.key !== key
              ? current
              : { ...current, loading: false, error: operationError(cause) },
          );
      })
      .finally(() => {
        request.busy = false;
      });
  }, [enabled, key, state, filters]);

  const current = state.key === key;
  return {
    items: current && enabled ? state.items : [],
    loading: enabled && (!current || state.loading),
    error: current && enabled ? state.error : "",
    hasMore: current && enabled && state.hasMore,
    loadMore,
    refresh: () => setRevision((value) => value + 1),
  };
}
