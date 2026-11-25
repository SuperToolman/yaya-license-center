"use client";
import { useOperation } from "../../../components/OperationProvider";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationData } from "../../../lib/operation-api";
import type { License, Order } from "../../../lib/operation-types";
export function useLicenses() {
  const { refreshToken, refresh } = useOperation();
  const query = useOperationQuery(
    "licenses",
    async (signal) => {
      const [licenses, orders] = await Promise.all([
        operationData<License[]>("/api/licenses", { signal }),
        operationData<Order[]>("/api/orders", { signal }),
      ]);
      return { licenses, orders };
    },
    refreshToken,
  );
  return {
    ...query,
    refresh,
    licenses: query.data?.licenses ?? [],
    orders: query.data?.orders ?? [],
  };
}
