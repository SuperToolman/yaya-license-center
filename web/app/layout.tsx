import type { Metadata } from "next";
import "./styles.css";

export const metadata: Metadata = { title: "Yaya 运营管理平台" };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="zh-CN"><body>{children}</body></html>;
}
