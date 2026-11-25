export type AuditLog = {
  id: number;
  createdAt: number;
  level: "debug" | "info" | "warning" | "error";
  source: string;
  endpointTitle: string;
  message: string;
  ipAddress?: string;
  method?: string;
  path?: string;
  statusCode?: number;
  durationMs?: number;
  userAgent?: string;
  responseBody?: string;
  customerId?: string;
  customerName?: string;
};
