"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { operationError } from "../lib/operation-api";

export function useOperationMutation() {
  const busy = useRef(false);
  const mounted = useRef(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const run = useCallback(
    async <T>(
      action: () => Promise<T>,
      onSuccess?: (value: T) => void,
    ): Promise<boolean> => {
      if (busy.current) return false;
      busy.current = true;
      setPending(true);
      setError("");
      try {
        const value = await action();
        if (mounted.current) onSuccess?.(value);
        return true;
      } catch (cause) {
        if (mounted.current) setError(operationError(cause));
        return false;
      } finally {
        busy.current = false;
        if (mounted.current) setPending(false);
      }
    },
    [],
  );
  return { run, pending, error, clearError: () => setError("") };
}
