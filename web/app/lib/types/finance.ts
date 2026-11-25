export type Transaction = {
  transactionId: string;
  orderId: string;
  orderNo: string;
  customerId: string;
  customerName: string;
  amountCents: number;
  method: string;
  reference: string;
  notes: string;
  occurredAt: number;
};

export type FinanceSummary = {
  totalReceivedCents: number;
  monthReceivedCents: number;
  outstandingCents: number;
  paidOrderCount: number;
  pendingOrderCount: number;
  activeCustomerCount: number;
};
