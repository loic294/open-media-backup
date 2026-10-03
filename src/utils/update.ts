import { formatBytes as baseFormatBytes, percent } from "./format";

export type ReleaseNoteBlock = { kind: "paragraph"; text: string } | { kind: "bullets"; items: string[] };

export function updateProgressPercent(downloaded: number, total: number | null | undefined): number | null {
  if (!total || total <= 0) return null;
  return percent(Math.max(0, downloaded), total);
}

export function formatUpdateProgress(downloaded: number, total: number | null | undefined): string {
  const downloadedText = baseFormatBytes(Math.max(0, downloaded));
  if (!total || total <= 0) return `${downloadedText} downloaded`;
  return `${downloadedText} of ${baseFormatBytes(total)}`;
}

export function parseReleaseNotes(notes: string | null | undefined): ReleaseNoteBlock[] {
  const text = notes?.replace(/\r\n?/g, "\n").trim();
  if (!text) return [];
  const blocks: ReleaseNoteBlock[] = [];
  let paragraph: string[] = [];
  let bullets: string[] = [];

  const flushParagraph = () => {
    if (!paragraph.length) return;
    blocks.push({ kind: "paragraph", text: paragraph.join("\n") });
    paragraph = [];
  };
  const flushBullets = () => {
    if (!bullets.length) return;
    blocks.push({ kind: "bullets", items: bullets });
    bullets = [];
  };

  for (const rawLine of text.split("\n")) {
    const line = rawLine.trim();
    if (!line) {
      flushParagraph();
      flushBullets();
      continue;
    }
    const bullet = line.match(/^(?:[-*•]\s+)(.+)$/);
    if (bullet) {
      flushParagraph();
      bullets.push(bullet[1].trim());
    } else {
      flushBullets();
      paragraph.push(rawLine.trimEnd());
    }
  }
  flushParagraph();
  flushBullets();
  return blocks;
}
