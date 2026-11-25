"use client";

import { useEffect, useRef, useState } from "react";
import { operationData, operationError } from "../lib/operation-api";

/** A stable query key owns the request lifecycle; the loader may close over that key. */
export function useOperationQuery<T>(
  key: string,
  load: (signal: AbortSignal) => Promise<T>,
  revision = 0,
) {
  const loader = useRef(load);
  useEffect(() => {
    loader.current = load;
  }, [load]);
  const [state, setState] = useState<{
    data: T | null;
    error: string;
    loading: boolean;
    key: string;
    requestId: string;
  }>({ data: null, error: "", loading: true, key, requestId: "" });
  const [retry, setRetry] = useState(0);
  const requestId = JSON.stringify([key, revision, retry]);
  useEffect(() => {
    const controller = new AbortController();
    void Promise.resolve()
      .then(() => {
        if (controller.signal.aborted) return;
        setState((current) => ({
          ...current,
          loading: true,
          error: "",
          requestId,
        }));
        return loader.current(controller.signal);
      })
      .then((data) => {
        if (!controller.signal.aborted && data !== undefined)
          setState({ data, error: "", loading: false, key, requestId });
      })
      .catch((cause) => {
        if (!controller.signal.aborted)
          setState((current) => ({
            ...current,
            error: operationError(cause),
            loading: false,
            key,
            requestId,
          }));
      });
    return () => controller.abort();
  }, [key, requestId]);
  return {
    data: state.key === key ? state.data : null,
    error: state.requestId === requestId ? state.error : "",
    loading: state.requestId !== requestId || state.loading,
    refresh: () => setRetry((value) => value + 1),
  };
}

export function useOperationResource<T>(path: string, revision = 0) {
  return useOperationQuery(
    path,
    (signal) => operationData<T>(path, { signal }),
    revision,
  );
}
