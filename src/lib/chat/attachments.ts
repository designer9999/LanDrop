/** Build history attachments from outgoing selections and incoming batch details. */
import type { MessageAttachment, SelectedFile } from "$lib/state/model";
import {
  fileNameFromPath,
  fileSizeStr,
  getReceivedFolderPath,
  isImage,
  isVideo,
  limitHistoryItems,
} from "$lib/utils/file-utils";

export const MAX_HISTORY_ITEMS = 100;

export function attachmentType(name: string): MessageAttachment["type"] {
  if (isImage(name)) return "image";
  if (isVideo(name)) return "video";
  return "file";
}

/** Attachments for an outgoing message, before anything is sent. */
export function attachmentsFromSelection(
  files: readonly SelectedFile[],
  historyPathFor: (path: string) => string = (path) => path,
): MessageAttachment[] {
  return limitHistoryItems([...files], MAX_HISTORY_ITEMS).map((file) => {
    const name = file.info?.name ?? fileNameFromPath(file.path, "file");
    const isFolder = file.info?.type === "folder";
    const attachment: MessageAttachment = {
      name,
      path: historyPathFor(file.path),
      size: file.info?.size_bytes ? fileSizeStr(file.info.size_bytes) : "",
      type: isFolder ? "folder" : attachmentType(name),
    };
    if (isFolder && file.info?.count) attachment.fileCount = file.info.count;
    return attachment;
  });
}

/** Group received files: top-level folders become one folder attachment each. */
export function attachmentsFromReceived(
  details: ReadonlyArray<{ name: string; path: string; size: number }>,
): MessageAttachment[] {
  const folders = new Map<string, Array<{ name: string; path: string; size: number }>>();
  const loose: Array<{ name: string; path: string; size: number }> = [];
  for (const detail of details) {
    const slash = detail.name.indexOf("/");
    if (slash > 0) {
      const folder = detail.name.slice(0, slash);
      const list = folders.get(folder) ?? [];
      list.push(detail);
      folders.set(folder, list);
    } else {
      loose.push(detail);
    }
  }

  const attachments: MessageAttachment[] = [];
  for (const [folder, entries] of folders) {
    const totalSize = entries.reduce((sum, entry) => sum + entry.size, 0);
    attachments.push({
      name: folder,
      path: getReceivedFolderPath(entries[0].path, entries[0].name),
      size: fileSizeStr(totalSize),
      type: "folder",
      fileCount: entries.length,
      children: limitHistoryItems(entries, MAX_HISTORY_ITEMS).map((entry) => ({
        name: entry.name,
        path: entry.path,
        size: fileSizeStr(entry.size),
        type: attachmentType(entry.name),
      })),
    });
  }
  for (const entry of loose) {
    attachments.push({
      name: entry.name,
      path: entry.path,
      size: fileSizeStr(entry.size),
      type: attachmentType(entry.name),
    });
  }
  return attachments;
}
