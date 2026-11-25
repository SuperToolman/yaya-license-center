import { test, expect } from "@playwright/test";
import { chipVariants } from "@heroui/styles";

test("变体 slot 函数必须捕获本次调用的 props", () => {
  const active = chipVariants({
    color: "success",
    size: "sm",
    variant: "soft",
  });
  const activeClass = active.base();
  const inactive = chipVariants({
    color: "default",
    size: "sm",
    variant: "soft",
  });
  expect(active).not.toBe(inactive);
  expect(active.base()).toBe(activeClass);
  expect(active.base()).toContain("chip--success");
  expect(inactive.base()).toContain("chip--default");
});
