import { zip } from "fflate";

export type SkillFile = { file: File; path: string };
const MAX_FILES = 256;
const MAX_BYTES = 20 * 1024 * 1024;

export async function packageSkillDirectory(files: SkillFile[]): Promise<File> {
  if (!files.length)
    throw new Error("文件夹为空，请选择包含 SKILL.md 的文件夹");
  if (files.length > MAX_FILES) throw new Error("文件数量不能超过 256 个");
  if (files.reduce((size, item) => size + item.file.size, 0) > MAX_BYTES)
    throw new Error("文件夹总大小不得超过 20 MB");
  const root = files[0].path.split("/")[0];
  const entries: Record<string, Uint8Array> = Object.create(null);
  for (const { file, path } of files) {
    const parts = path.split("/");
    if (
      parts[0] !== root ||
      parts.length < 2 ||
      parts.some(
        (part) =>
          !part || part === "." || part === ".." || /[\\:\0]/.test(part),
      )
    )
      throw new Error("请选择一个 Skill 文件夹，文件路径不能包含特殊字符");
    const relative = parts.slice(1).join("/");
    if (relative in entries) throw new Error("文件夹中存在重复文件路径");
    entries[relative] = new Uint8Array(await file.arrayBuffer());
  }
  if (!("SKILL.md" in entries))
    throw new Error("文件夹根目录必须包含 SKILL.md 文件");
  const bytes = await new Promise<Uint8Array<ArrayBuffer>>(
    (resolve, reject) => {
      zip(entries, { level: 6 }, (error, data) => {
        if (error) reject(error);
        else resolve(data as Uint8Array<ArrayBuffer>);
      });
    },
  );
  return new File([bytes], `${root}.zip`, { type: "application/zip" });
}

export async function readDroppedSkill(
  items: DataTransferItem[],
): Promise<SkillFile[]> {
  const entries = items
    .filter((item) => item.kind === "file")
    .map((item) => item.webkitGetAsEntry());
  if (entries.length !== 1 || !entries[0]?.isDirectory)
    throw new Error("请拖入一个 ZIP 文件或一个 Skill 文件夹");
  const files: SkillFile[] = [];
  let size = 0;
  async function visit(entry: FileSystemEntry) {
    if (entry.isFile) {
      const file = await new Promise<File>((resolve, reject) =>
        (entry as FileSystemFileEntry).file(resolve, reject),
      );
      size += file.size;
      if (files.length >= MAX_FILES || size > MAX_BYTES)
        throw new Error("文件夹不能超过 256 个文件或 20 MB");
      files.push({ file, path: entry.fullPath.replace(/^\//, "") });
    } else {
      const reader = (entry as FileSystemDirectoryEntry).createReader();
      while (true) {
        const batch = await new Promise<FileSystemEntry[]>((resolve, reject) =>
          reader.readEntries(resolve, reject),
        );
        if (!batch.length) break;
        for (const child of batch) await visit(child);
      }
    }
  }
  await visit(entries[0]);
  return files;
}
