export type LicenseStatus = "unactivated" | "running" | "expired" | "destroyed";

export type License = {
  license: string;
  licenseId: string;
  orderId?: string;
  subject: string;
  customerNameSnapshot?: string;
  linkageStatus?: "linked" | "legacy_unlinked";
  modules: string[];
  issuedAt: number;
  expiresAt: number;
  moduleExpiresAt: Record<string, number>;
  moduleTitles: Record<string, string>;
  aiEmployees: Array<{ id: string; title: string; expiresAt: number }>;
  aiEmployeeStatuses: Record<string, LicenseStatus>;
  platformStatus: LicenseStatus;
  moduleStatuses: Record<string, LicenseStatus>;
};

export type CustomerAuthorization = {
  authorizationId: string;
  customerId: string;
  latestLicenseId: string | null;
  activeLicenseId: string | null;
  revision: number;
  latest: License | null;
  versions: Array<{
    license: License;
    state: "pending" | "active" | "superseded" | "revoked";
    revision: number;
  }>;
};
