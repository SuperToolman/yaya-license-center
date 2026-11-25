import { test, expect } from "@playwright/test";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { unzipSync, strFromU8, zipSync, strToU8 } from "fflate";
import { mockApi } from "./fixtures";

test("编辑 Skill 复用导入弹窗更新版本，失败保留编辑内容，文件夹重试更新原 Skill", async ({
  page,
}) => {
  const state = await mockApi(page);
  await page.goto("/skills");
  await page.locator('button[aria-label="编辑 bom-quote"]').click();
  const edit = page.getByRole("dialog", { name: "编辑 Skill", exact: true });
  await edit.getByLabel("Skill 名称", { exact: true }).fill("保留编辑内容");
  const importButton = edit.getByRole("button", {
    name: "导入新版本",
    exact: true,
  });
  const saveButton = edit.getByRole("button", {
    name: "保存 Skill",
    exact: true,
  });
  const importBounds = await importButton.boundingBox();
  const saveBounds = await saveButton.boundingBox();
  expect(importBounds!.x).toBeLessThan(saveBounds!.x);
  expect(Math.abs(importBounds!.y - saveBounds!.y)).toBeLessThan(2);
  let choosers = 0;
  page.on("filechooser", () => choosers++);
  await importButton.click();
  const dialog = page.getByRole("dialog", { name: "导入新版本", exact: true });
  await expect(dialog).toBeVisible();
  expect(choosers).toBe(0);
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(edit.getByLabel("Skill 名称", { exact: true })).toHaveValue(
    "保留编辑内容",
  );
  await importButton.click();
  state.fail = "/api/skills/bom-quote/archive";
  await dialog
    .getByLabel("ZIP 文件", { exact: true })
    .setInputFiles({
      name: "update.zip",
      mimeType: "application/zip",
      buffer: Buffer.from(zipSync({ "SKILL.md": strToU8("# Updated") })),
    });
  await expect(dialog.getByText("模拟请求失败", { exact: true })).toBeVisible();
  state.fail = "";
  const root = await mkdtemp(join(tmpdir(), "skill-version-"));
  try {
    await writeFile(join(root, "SKILL.md"), "# Folder update");
    await dialog
      .getByLabel("Skill 文件夹", { exact: true })
      .setInputFiles(root);
    await expect(dialog).toHaveCount(0);
    await expect(edit).toHaveCount(0);
    expect(
      state.calls.filter(
        (call) =>
          call.path === "/api/skills/bom-quote/archive" &&
          call.method === "POST",
      ),
    ).toHaveLength(2);
    expect(
      state.calls.filter((call) => call.path === "/api/skills/import"),
    ).toHaveLength(0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("文件夹导入校验 SKILL.md，保留子目录和二进制文件", async ({
  page,
}, testInfo) => {
  const state = await mockApi(page);
  const root = await mkdtemp(join(tmpdir(), "skill-import-"));
  const directory = join(root, "demo-skill");
  try {
    await mkdir(join(directory, "assets"), { recursive: true });
    const binary = Buffer.from([0, 255, 128, 1]);
    await writeFile(join(directory, "assets", "sample.bin"), binary);
    await page.goto("/skills");
    let chooserCount = 0;
    page.on("filechooser", () => chooserCount++);
    await page.getByRole("button", { name: "导入", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "导入技能" });
    await expect(dialog).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("import-modal.png"),
      animations: "disabled",
    });
    expect(chooserCount).toBe(0);
    await dialog
      .getByLabel("Skill 文件夹", { exact: true })
      .setInputFiles(directory);
    await expect(
      dialog.getByText("文件夹根目录必须包含 SKILL.md 文件"),
    ).toBeVisible();
    expect(
      state.calls.filter((call) => call.path === "/api/skills/import"),
    ).toHaveLength(0);
    const instructions =
      "---\nname: demo-skill\ndescription: 测试技能\n---\n# 测试";
    await writeFile(join(directory, "SKILL.md"), instructions);
    const requestPromise = page.waitForRequest(
      (request) =>
        request.url().endsWith("/api/skills/import") &&
        request.method() === "POST",
    );
    await dialog
      .getByLabel("Skill 文件夹", { exact: true })
      .setInputFiles(directory);
    const request = await requestPromise;
    const body = request.postDataBuffer()!;
    const boundary = request.headers()["content-type"].split("boundary=")[1];
    const start = body.indexOf("\r\n\r\n") + 4;
    const end = body.indexOf(`\r\n--${boundary}`, start);
    const entries = unzipSync(body.subarray(start, end));
    expect(Object.keys(entries).sort()).toEqual([
      "SKILL.md",
      "assets/sample.bin",
    ]);
    expect(strFromU8(entries["SKILL.md"])).toBe(instructions);
    expect(Buffer.from(entries["assets/sample.bin"])).toEqual(binary);
    await expect(dialog).toHaveCount(0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("ZIP 拖拽失败保留弹窗，重试成功关闭", async ({ page }) => {
  const state = await mockApi(page);
  state.fail = "/api/skills/import";
  await page.goto("/skills");
  await page.getByRole("button", { name: "导入", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "导入技能" });
  const bytes = Array.from(zipSync({ "SKILL.md": strToU8("# Demo") }));
  const data = await page.evaluateHandle((bytes) => {
    const transfer = new DataTransfer();
    transfer.items.add(
      new File([new Uint8Array(bytes)], "demo.zip", {
        type: "application/zip",
      }),
    );
    return transfer;
  }, bytes);
  await dialog
    .getByRole("button", { name: /拖拽 ZIP/ })
    .dispatchEvent("drop", { dataTransfer: data });
  await expect(dialog.getByText("模拟请求失败")).toBeVisible();
  state.fail = "";
  await dialog.getByLabel("ZIP 文件", { exact: true }).setInputFiles({
    name: "demo.zip",
    mimeType: "application/zip",
    buffer: Buffer.from(bytes),
  });
  await expect(dialog).toHaveCount(0);
  expect(
    state.calls.filter(
      (call) => call.path === "/api/skills/import" && call.method === "POST",
    ),
  ).toHaveLength(2);
});
