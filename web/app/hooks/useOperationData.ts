"use client";

import { useOperation } from "../components/OperationProvider";
import { useOperationResource } from "./useOperationQuery";

/** Page resources subscribe only to the existing explicit refresh event. */
export function useOperationData<T>(path: string) {
  const { refreshToken, refresh } = useOperation();
  return { ...useOperationResource<T>(path, refreshToken), refresh };
}
