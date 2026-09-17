import OperationAuthGate from "../components/OperationAuthGate";
import OperationPageLayout from "../components/OperationPageLayout";
import { OperationProvider } from "../components/OperationProvider";
export default function OperationLayout({ children }: { children: React.ReactNode }) { return <OperationProvider><OperationAuthGate><OperationPageLayout>{children}</OperationPageLayout></OperationAuthGate></OperationProvider>; }
