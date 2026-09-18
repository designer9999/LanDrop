import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const tauriRoot = resolve(process.cwd(), "src-tauri");
const read = (path: string) => readFileSync(resolve(tauriRoot, path), "utf8");
const grants = [
  "file-helper:allow-open-file",
  "file-helper:allow-save-to-downloads",
  "file-helper:allow-get-file-name",
];

describe("Android file-helper permission contract", () => {
  it("grants only the three attachment commands to the local Android main window", () => {
    const mobile = JSON.parse(read("capabilities/mobile.json"));
    expect(mobile.platforms).toEqual(["android"]);
    expect(mobile.windows).toEqual(["main"]);
    expect(mobile.remote).toBeUndefined();
    expect(mobile.local ?? true).toBe(true);
    expect(
      mobile.permissions.filter(
        (p: unknown) => typeof p === "string" && p.startsWith("file-helper:"),
      ),
    ).toEqual(grants);
    for (const file of readdirSync(resolve(tauriRoot, "capabilities"))) {
      if (file.endsWith(".json") && file !== "mobile.json") {
        expect(read(`capabilities/${file}`)).not.toContain("file-helper:");
      }
    }
  });

  it("uses exact native command names without a wildcard or default grant", () => {
    const permissions = read("permissions/file-helper/attachments.toml");
    const rules = [
      ...permissions.matchAll(/identifier = "([^"]+)"[\s\S]*?commands\.allow = \["([^"]+)"\]/g),
    ].map((match) => [match[1], match[2]]);
    expect(rules).toEqual([
      ["allow-open-file", "openFile"],
      ["allow-save-to-downloads", "saveToDownloads"],
      ["allow-get-file-name", "getFileName"],
    ]);
    expect(permissions).not.toContain("[default]");
    expect(permissions).not.toContain("*");
    expect(permissions).not.toContain("getThumbnail");
    expect(permissions).not.toContain("saveDocumentResult");
  });

  it("registers the inlined manifest that makes the native grants resolvable", () => {
    const build = read("build.rs");
    expect(build).toMatch(/\.plugin\(\s*"file-helper",\s*tauri_build::InlinedPlugin::new\(\)/);
    expect(build).not.toContain("AllowAllCommands");
  });
});
