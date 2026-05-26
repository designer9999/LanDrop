export const IMAGE_EXTS = new Set([".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".ico", ".svg"]);
export const VIDEO_EXTS = new Set([".mp4", ".webm", ".mov", ".avi", ".mkv", ".m4v", ".ogv"]);

export function fileNameFromPath(path: string, fallback = ""): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? fallback;
}

export function fileExtension(nameOrExt: string): string {
  const name = fileNameFromPath(nameOrExt, nameOrExt).trim().toLowerCase();
  if (!name) return "";
  if (name.startsWith(".") && !name.slice(1).includes(".")) return name;
  const dot = name.lastIndexOf(".");
  if (dot < 0 || dot === name.length - 1) return "";
  return name.slice(dot);
}

export function fileExtensionLabel(nameOrExt: string, fallback = "FILE"): string {
  return fileExtension(nameOrExt).replace(/^\./, "").toUpperCase() || fallback;
}

export function isImage(nameOrExt: string): boolean {
  return IMAGE_EXTS.has(fileExtension(nameOrExt));
}

export function isVideo(nameOrExt: string): boolean {
  return VIDEO_EXTS.has(fileExtension(nameOrExt));
}

export function imageMimeFromName(nameOrExt: string): string {
  switch (fileExtension(nameOrExt)) {
    case ".png":
      return "image/png";
    case ".gif":
      return "image/gif";
    case ".webp":
      return "image/webp";
    case ".bmp":
      return "image/bmp";
    case ".ico":
      return "image/x-icon";
    case ".svg":
      return "image/svg+xml";
    default:
      return "image/jpeg";
  }
}

export function videoMimeFromName(nameOrExt: string): string {
  switch (fileExtension(nameOrExt)) {
    case ".webm":
      return "video/webm";
    case ".mov":
      return "video/quicktime";
    case ".mkv":
      return "video/x-matroska";
    case ".avi":
      return "video/x-msvideo";
    case ".ogv":
      return "video/ogg";
    case ".m4v":
      return "video/x-m4v";
    default:
      return "video/mp4";
  }
}

export function fileSizeStr(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
