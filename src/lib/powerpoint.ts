import { toMarkdown } from "@mdgate/ppt";

const MAX_SLIDES_BYTES = 50 * 1024 * 1024;
const MAX_SLIDES = 2_000;

export function isSupportedSlidesFileName(name: string) {
  return /\.(pdf|pptx|ppt)$/i.test(name.trim());
}

export function isLegacyPowerPointFileName(name: string) {
  return /\.ppt$/i.test(name.trim());
}

export function slidesMimeType(name: string, browserMimeType = "") {
  if (browserMimeType.trim()) return browserMimeType;
  const lower = name.toLowerCase();
  if (lower.endsWith(".pdf")) return "application/pdf";
  if (lower.endsWith(".ppt")) return "application/vnd.ms-powerpoint";
  return "application/vnd.openxmlformats-officedocument.presentationml.presentation";
}

export function splitLegacyPowerPointMarkdown(markdown: string) {
  const normalized = markdown.replace(/\r\n?/g, "\n").trim();
  if (!normalized) return [];

  const pages: string[] = [];
  let current: string[] = [];
  for (const line of normalized.split("\n")) {
    if (/^##\s+\S/.test(line) && current.some((item) => item.trim())) {
      pages.push(current.join("\n").trim());
      current = [];
    }
    current.push(line);
  }
  if (current.some((item) => item.trim())) pages.push(current.join("\n").trim());
  return pages.slice(0, MAX_SLIDES);
}

export async function parseLegacyPowerPointPages(file: File) {
  if (!isLegacyPowerPointFileName(file.name)) {
    throw new Error("所选文件不是 PowerPoint 97-2003 PPT");
  }
  if (file.size <= 0 || file.size > MAX_SLIDES_BYTES) {
    throw new Error("Slides 文件必须小于 50MB");
  }

  let markdown: string;
  try {
    markdown = await toMarkdown(new Uint8Array(await file.arrayBuffer()), { path: file.name });
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : String(reason);
    throw new Error(`无法读取这个旧版 PPT：${message}`);
  }
  const pages = splitLegacyPowerPointMarkdown(markdown);
  if (pages.length === 0) {
    throw new Error("PPT 中没有找到可读取的幻灯片");
  }
  return pages;
}
