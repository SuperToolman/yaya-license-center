import FilesPage from "../../../components/ops/FilesPage";
import { Suspense } from "react";
export default function Page() {
  return (
    <Suspense fallback={<p>正在加载文件管理…</p>}>
      <FilesPage />
    </Suspense>
  );
}
