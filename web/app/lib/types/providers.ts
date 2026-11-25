export type Provider = {
  providerId: string;
  name: string;
  contactName: string;
  contactPhone: string;
  status: string;
  createdAt: number;
};

export type CommissionRule = {
  ruleId: string;
  productType: string;
  rateBasisPoints: number;
  status: string;
  effectiveAt: number;
};

export type CommissionEntry = {
  entryId: string;
  orderId: string;
  providerId: string;
  productType: string;
  receivedAmountCents: number;
  rateBasisPoints: number;
  commissionAmountCents: number;
  status: string;
  createdAt: number;
};

export type Settlement = {
  batchId: string;
  totalAmountCents: number;
  entryCount: number;
  status: string;
  settledAt: number;
  notes: string;
};
