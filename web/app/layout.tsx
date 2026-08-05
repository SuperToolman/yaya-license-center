import type { Metadata } from "next";
import "./styles.css";

export const metadata: Metadata = { title: "丫丫许可证中心" };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="zh-CN"><body>{children}</body></html>;
}
