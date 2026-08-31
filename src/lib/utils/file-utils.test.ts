import { describe, expect, it } from "vitest";
import {
  fileExtension,
  fileExtensionLabel,
  fileNameFromPath,
  fileSizeStr,
  getReceivedFolderPath,
  imageMimeFromName,
  isImage,
  isVideo,
  joinReceivePath,
  limitHistoryItems,
  videoMimeFromName,
} from "./file-utils";

describe("fileNameFromPath", () => {
  it("takes the last segment of either separator style", () => {
    expect(fileNameFromPath("C:\\dl\\a.txt")).toBe("a.txt");
    expect(fileNameFromPath("/home/u/dl/a.txt")).toBe("a.txt");
    expect(fileNameFromPath("a.txt")).toBe("a.txt");
  });

  it("falls back when there is no segment", () => {
    expect(fileNameFromPath("", "file")).toBe("file");
    expect(fileNameFromPath("///", "file")).toBe("file");
  });
});

describe("fileExtension", () => {
  it("returns only the final extension of a multi-dot name", () => {
    expect(fileExtension("archive.tar.gz")).toBe(".gz");
  });

  it("treats a leading-dot name as the extension itself", () => {
    expect(fileExtension(".gitignore")).toBe(".gitignore");
  });

  it("returns empty for a trailing dot or no dot at all", () => {
    expect(fileExtension("name.")).toBe("");
    expect(fileExtension("noext")).toBe("");
    expect(fileExtension("")).toBe("");
  });

  it("accepts a bare extension and normalizes case", () => {
    expect(fileExtension(".PNG")).toBe(".png");
    expect(fileExtension("Photo.JPEG")).toBe(".jpeg");
  });

  it("uses the file name, not a dotted directory", () => {
    expect(fileExtension("/home/u/.config/readme")).toBe("");
  });
});

describe("fileExtensionLabel", () => {
  it("uppercases without the dot and falls back", () => {
    expect(fileExtensionLabel("a.pdf")).toBe("PDF");
    expect(fileExtensionLabel("noext")).toBe("FILE");
    expect(fileExtensionLabel("noext", "DOC")).toBe("DOC");
  });
});

describe("media type detection", () => {
  it("classifies images and videos", () => {
    expect(isImage("a.PNG")).toBe(true);
    expect(isImage(".webp")).toBe(true);
    expect(isImage("a.mp4")).toBe(false);
    expect(isVideo("clip.MKV")).toBe(true);
    expect(isVideo("a.png")).toBe(false);
  });

  it("maps mime types with sensible defaults", () => {
    expect(imageMimeFromName("a.png")).toBe("image/png");
    expect(imageMimeFromName("a.svg")).toBe("image/svg+xml");
    expect(imageMimeFromName("a.unknown")).toBe("image/jpeg");
    expect(videoMimeFromName("a.webm")).toBe("video/webm");
    expect(videoMimeFromName("a.mov")).toBe("video/quicktime");
    expect(videoMimeFromName("a.unknown")).toBe("video/mp4");
  });
});

describe("fileSizeStr", () => {
  it("switches unit exactly at each 1024 boundary", () => {
    expect(fileSizeStr(0)).toBe("0 B");
    expect(fileSizeStr(1023)).toBe("1023 B");
    expect(fileSizeStr(1024)).toBe("1.0 KB");
    expect(fileSizeStr(1024 * 1024 - 1)).toBe("1024.0 KB");
    expect(fileSizeStr(1024 * 1024)).toBe("1.0 MB");
    expect(fileSizeStr(1024 * 1024 * 1024 - 1)).toBe("1024.0 MB");
    expect(fileSizeStr(1024 * 1024 * 1024)).toBe("1.0 GB");
  });
});

describe("getReceivedFolderPath", () => {
  it("walks up one level per extra path segment", () => {
    expect(getReceivedFolderPath("C:\\dl\\docs\\a.txt", "docs/a.txt")).toBe("C:\\dl\\docs");
    expect(getReceivedFolderPath("C:\\dl\\top\\sub\\a.txt", "top/sub/a.txt")).toBe("C:\\dl\\top");
    expect(getReceivedFolderPath("/home/u/dl/docs/a.txt", "docs/a.txt")).toBe("/home/u/dl/docs");
  });

  it("leaves a loose file path untouched", () => {
    expect(getReceivedFolderPath("C:\\dl\\a.txt", "a.txt")).toBe("C:\\dl\\a.txt");
  });
});

describe("joinReceivePath", () => {
  it("uses the base folder's separator style", () => {
    expect(joinReceivePath("C:\\dl", "a/b.txt")).toBe("C:\\dl\\a\\b.txt");
    expect(joinReceivePath("/home/u/dl", "a/b.txt")).toBe("/home/u/dl/a/b.txt");
  });

  it("normalizes trailing and leading separators", () => {
    expect(joinReceivePath("C:\\dl\\\\", "\\a.txt")).toBe("C:\\dl\\a.txt");
    expect(joinReceivePath("/home/u/dl/", "/a.txt")).toBe("/home/u/dl/a.txt");
  });

  it("returns the name unchanged when there is no base", () => {
    expect(joinReceivePath("", "a.txt")).toBe("a.txt");
  });
});

describe("limitHistoryItems", () => {
  it("keeps the first N items and passes short lists through", () => {
    expect(limitHistoryItems([1, 2, 3], 2)).toEqual([1, 2]);
    const short = [1, 2];
    expect(limitHistoryItems(short, 5)).toBe(short);
  });
});
