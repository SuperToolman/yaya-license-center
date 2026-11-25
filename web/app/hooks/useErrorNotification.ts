"use client";

import { toast } from "@heroui/react";
import { useEffect, useRef } from "react";

/** Notify once per error; a successful request allows the same error to notify again. */
export function useErrorNotification(
  error: string | null | undefined,
  loading = false,
) {
  const previous = useRef("");
  useEffect(() => {
    if (loading) return;
    if (error && error !== previous.current) {
      toast.danger(error, { timeout: 8000 });
    }
    previous.current = error ?? "";
  }, [error, loading]);
}
