"use client";
import { useOperation } from "../../../components/OperationProvider";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationData } from "../../../lib/operation-api";
import type {
  AiEmployeeProduct,
  Customer,
  License,
  Order,
  Provider,
} from "../../../lib/operation-types";
export function useOrders() {
  const { refreshToken, refresh } = useOperation();
  const query = useOperationQuery(
    "orders",
    async (signal) => {
      const [orders, customers, employees, providers, licenses] =
        await Promise.all([
          operationData<Order[]>("/api/orders", { signal }),
          operationData<Customer[]>("/api/customers", { signal }),
          operationData<AiEmployeeProduct[]>("/api/ai-employees", { signal }),
          operationData<Provider[]>("/api/providers", { signal }),
          operationData<License[]>("/api/licenses", { signal }),
        ]);
      return { orders, customers, employees, providers, licenses };
    },
    refreshToken,
  );
  return {
    ...query,
    refresh,
    orders: query.data?.orders ?? [],
    customers: query.data?.customers ?? [],
    employees: query.data?.employees ?? [],
    providers: query.data?.providers ?? [],
    licenses: query.data?.licenses ?? [],
  };
}
