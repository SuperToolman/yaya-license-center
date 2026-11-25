"use client";

import { usePathname } from "next/navigation";
import OperationSidebar from "./OperationSidebar";
export default function OperationPageLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const isOrdersPage = usePathname() === "/orders";
  return (
    <main
      className={`${isOrdersPage ? "flex h-dvh flex-col overflow-hidden max-[760px]:[&>aside]:max-h-[20dvh] max-[760px]:[&>aside]:shrink-0 max-[760px]:[&>aside]:overflow-auto" : "min-h-screen"} min-[761px]:grid min-[761px]:grid-cols-[224px_minmax(0,1fr)]`}
    >
      <OperationSidebar />
      <section
        className={`${isOrdersPage ? "flex min-h-0 flex-1 flex-col" : ""} min-w-0 px-4 py-7 sm:px-8 lg:px-10`}
      >
        <div
          className={`${isOrdersPage ? "flex min-h-0 flex-1 flex-col" : ""} mx-auto w-full max-w-[1440px]`}
        >
          {children}
        </div>
      </section>
    </main>
  );
}
