import type { Metadata } from "next";
import GlobalNotifications from "./components/GlobalNotifications";
import "./styles.css";
import "@xterm/xterm/css/xterm.css";

export const metadata: Metadata = { title: "Yaya 运营管理平台" };

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body style={{ zoom: 1 }}>
        {children}
        <GlobalNotifications />
      </body>
    </html>
  );
}
