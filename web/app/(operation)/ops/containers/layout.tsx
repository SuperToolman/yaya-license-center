import { Suspense, type ReactNode } from "react";
import LocalContainersPage from "../../../components/ops/LocalContainersPage";

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <>
      {children}
      <Suspense fallback={<p>正在加载容器管理…</p>}>
        <LocalContainersPage />
      </Suspense>
    </>
  );
}
