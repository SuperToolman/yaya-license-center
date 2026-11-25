import { notFound } from "next/navigation";
import { dockerTabs } from "../../../../components/ops/docker-routes";

export default async function Page({
  params,
}: {
  params: Promise<{ tab: string }>;
}) {
  const { tab } = await params;
  if (!dockerTabs.some(([id]) => id === tab)) notFound();
  return null;
}
