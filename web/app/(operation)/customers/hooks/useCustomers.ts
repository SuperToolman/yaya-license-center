"use client";
import { useOperation } from "../../../components/OperationProvider";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationData } from "../../../lib/operation-api";
import type { Customer, Order } from "../../../lib/operation-types";
export function useCustomers() {
  const { refreshToken, refresh } = useOperation();
  const query = useOperationQuery(
    "customers",
    async (signal) => {
      const [customers, orders] = await Promise.all([
        operationData<Customer[]>("/api/customers", { signal }),
        operationData<Order[]>("/api/orders", { signal }),
      ]);
      return { customers, orders };
    },
    refreshToken,
  );
  return {
    ...query,
    refresh,
    customers: query.data?.customers ?? [],
    orders: query.data?.orders ?? [],
  };
}
