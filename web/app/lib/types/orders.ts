export type OrderItem = {
  kind: "platform" | "module" | "ai_employee" | "service" | "application";
  referenceId?: string;
  title: string;
  quantity: number;
  unitPriceCents: number;
  entitlementDays?: number;
};

export type Order = {
  orderId: string;
  paymentStatus?:"unpaid"|"partial"|"paid"|"cancelled";
  fulfillmentStatus?:"awaiting_payment"|"pending"|"completed"|"cancelled";
  orderNo: string;
  customerId: string;
  customerName: string;
  items: OrderItem[];
  totalAmountCents: number;
  receivedAmountCents: number;
  status: "pending_payment" | "paid" | "fulfilled" | "cancelled";
  createdAt: number;
  dueAt?: number;
  deploymentType: "saas" | "local";
  paidAt?: number;
  licenseId?: string;
  notes: string;
  providerId?: string;
};
