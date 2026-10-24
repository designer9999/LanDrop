/**
 * Tokenise message text into plain text, links, inline code and fenced code
 * blocks. Rendering stays text-only (no HTML injection): components map each
 * token to elements, so a peer can never smuggle markup into the webview.
 */
export type RichToken =
  | { type: "text"; value: string }
  | { type: "link"; value: string; href: string }
  | { type: "code"; value: string }
  | { type: "codeblock"; value: string; lang: string };

const FENCE = /```([\w+#.-]*)[ \t]*\n?([\s\S]*?)```/g;
const INLINE = /`([^`\n]+)`/g;
// http(s) URLs and bare www. hosts. Trailing punctuation is trimmed below.
const URL_RE = /\b(?:https?:\/\/|www\.)[^\s<>"'`]+/gi;

function trimUrl(raw: string): string {
  let url = raw;
  while (/[.,;:!?)\]}'"]$/.test(url)) {
    // Keep a closing parenthesis that balances one inside the URL (Wikipedia links).
    if (url.endsWith(")") && (url.match(/\(/g)?.length ?? 0) >= (url.match(/\)/g)?.length ?? 0)) {
      break;
    }
    url = url.slice(0, -1);
  }
  return url;
}

function linkify(text: string, out: RichToken[]) {
  let last = 0;
  for (const match of text.matchAll(URL_RE)) {
    const start = match.index ?? 0;
    const url = trimUrl(match[0]);
    if (start > last) out.push({ type: "text", value: text.slice(last, start) });
    const href = url.toLowerCase().startsWith("www.") ? `https://${url}` : url;
    out.push({ type: "link", value: url, href });
    last = start + url.length;
  }
  if (last < text.length) out.push({ type: "text", value: text.slice(last) });
}

function inline(text: string, out: RichToken[]) {
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const start = match.index ?? 0;
    if (start > last) linkify(text.slice(last, start), out);
    out.push({ type: "code", value: match[1] });
    last = start + match[0].length;
  }
  if (last < text.length) linkify(text.slice(last), out);
}

export function tokenizeMessage(text: string): RichToken[] {
  const out: RichToken[] = [];
  let last = 0;
  for (const match of text.matchAll(FENCE)) {
    const start = match.index ?? 0;
    if (start > last) inline(text.slice(last, start).replace(/\n$/, ""), out);
    out.push({ type: "codeblock", lang: match[1] ?? "", value: match[2].replace(/\n$/, "") });
    last = start + match[0].length;
    if (text[last] === "\n") last += 1;
  }
  if (last < text.length) inline(text.slice(last), out);
  return out.filter((token) => token.type !== "text" || token.value.length > 0);
}

/** A message made only of emoji (up to three) is shown larger, like most messengers. */
export function isEmojiOnly(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed || trimmed.length > 24) return false;
  const stripped = trimmed.replace(/\s/g, "");
  if (!/^(?:\p{Extended_Pictographic}|\p{Emoji_Component}|\u200d|\ufe0f)+$/u.test(stripped)) {
    return false;
  }
  const segments = [
    ...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(stripped),
  ];
  return segments.length > 0 && segments.length <= 3 && !/^[\d#*]+$/.test(stripped);
}
