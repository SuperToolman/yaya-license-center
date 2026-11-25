export type Customer = {
  customerId: string;
  name: string;
  contactName: string;
  contactPhone: string;
  contactEmail: string;
  status: "active" | "inactive";
  notes: string;
  createdAt: number;
};

export type CustomerDeletionEligibility = {
  canDelete: boolean;
  blockers: Array<{
    kind: string;
    label: string;
    count: number;
    recordIds: string[];
  }>;
};
