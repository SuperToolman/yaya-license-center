"use client";

import { Toast } from "@heroui/react";

export default function GlobalNotifications() {
  return <Toast.Provider placement="top end" maxVisibleToasts={3} />;
}
