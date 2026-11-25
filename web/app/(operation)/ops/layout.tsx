import { OpsProvider } from "../../components/ops/OpsProvider";

export default function OpsLayout({ children }: { children: React.ReactNode }) {
  return <OpsProvider>{children}</OpsProvider>;
}
