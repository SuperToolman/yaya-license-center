export type CommercialConfig = { pricingMode: "free" | "paid"; priceCents: number; publisherRateBasisPoints: number; currency: string; licenseModel: string; description: string };
export type CommercialVersion = { id: string; releaseId: string; config: CommercialConfig; status: string; reason: string };
export type ApplicationReleaseSubmission = {
  submissionId: string;
  appId: string;
  appName: string;
  version: string;
  status: string;
  submittedBy: string;
  applicantSubject: string;
  submittedAt: number;
  productId?: string;
  reviewReason?: string;
  snapshot: { submissionType?: "new_listing" | "version_upgrade"; source?: string; targetProductId?: string; commercial?: CommercialConfig; manifest?: { applicationType: "single" | "suite"; selectedAppIds?: string[]; members: { appId: string; resources: { apps: { name: string }[] } }[]; dependencies: { sourceAppId: string; targetAppId: string; path: string; resourceId: string }[] } };

};

export type ApplicationProduct = {
  productId: string;
  commercialReady?: boolean;
  commercialId?: string;
  pricingMode?: "free" | "paid";
  priceCents?: number;
  publisherRateBasisPoints?: number;
  publisherSubject: string;
  name: string;
  description: string;
  releaseId?: string | null;
  version?: string | null;
  listingStatus: string;
  entitlementCount: number;
  applicationType: "single" | "suite";
  selectedAppIds?: string[];
  members: { appId: string; name: string }[];
  dependencies: { sourceAppId: string; targetAppId: string; path: string; resourceId: string }[];
  pendingCount: number;
};
