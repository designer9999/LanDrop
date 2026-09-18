#!/usr/bin/env node
// Android release gate: ZIP alignment alone does not establish ELF compatibility.
// https://developer.android.com/guide/practices/page-sizes
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const PAGE_SIZE = 0x4000n;

export function verifyReadelf(output, label = "ELF library") {
  if (
    !/^\s*Class:\s+ELF64\s*$/m.test(output) ||
    !/^\s*Machine:\s+AArch64\s*$/m.test(output) ||
    !/^\s*Type:\s+DYN\b/m.test(output)
  ) {
    throw new Error(`${label}: expected an ELF64 AArch64 shared library`);
  }
  const failures = [];
  let loadCount = 0;
  let relroCount = 0;
  for (const line of output.split(/\r?\n/)) {
    if (!/^\s*(LOAD|GNU_RELRO)\b/.test(line)) continue;
    const match = line.match(
      /^\s*(LOAD|GNU_RELRO)\s+(0x[\da-f]+)\s+(0x[\da-f]+)\s+(0x[\da-f]+)\s+(0x[\da-f]+)\s+(0x[\da-f]+)\s+[RWE ]+\s+(0x[\da-f]+)\s*$/i,
    );
    if (!match) throw new Error(`${label}: unrecognized program-header format`);
    const [, type, offsetText, virtualText, , , memoryText, alignmentText] = match;
    const offset = BigInt(offsetText);
    const virtual = BigInt(virtualText);
    const memory = BigInt(memoryText);
    const alignment = BigInt(alignmentText);
    if (type === "LOAD") {
      loadCount++;
      if (alignment < PAGE_SIZE || (alignment & (alignment - 1n)) !== 0n) {
        failures.push(`LOAD alignment ${alignmentText} is not a power of two >=0x4000`);
      } else if (offset % alignment !== virtual % alignment) {
        failures.push("LOAD file offset and virtual address are not congruent for their alignment");
      }
    } else {
      relroCount++;
      const remainder = (virtual + memory) % PAGE_SIZE;
      if (remainder !== 0n) {
        failures.push(`GNU_RELRO end is not 16 KB aligned (remainder 0x${remainder.toString(16)})`);
      }
    }
  }
  if (loadCount === 0) failures.push("no LOAD segments found");
  // No RELRO segment is trivially page-compatible, not evidence of RELRO hardening.
  if (failures.length) throw new Error(`${label}: ${failures.join("; ")}`);
  return { loadCount, relroCount };
}

export function selectArm64Libraries(listing) {
  const entries = listing.split(/\r?\n/).filter(Boolean);
  const selected = entries.filter((entry) => entry.startsWith("lib/") && entry.endsWith(".so"));
  if (selected.length === 0 || selected.length > 128) {
    throw new Error("APK must contain between 1 and 128 arm64 shared libraries");
  }
  for (const entry of selected) {
    if (!/^lib\/arm64-v8a\/[A-Za-z0-9_.+-]+\.so$/.test(entry)) {
      throw new Error("APK contains an unexpected ABI or unsafe native-library entry");
    }
    // Reject duplicate ZIP records and any directory masquerading as a library.
    if (
      entries.filter((other) => other === entry).length !== 1 ||
      entries.some((other) => other.startsWith(`${entry}/`))
    ) {
      throw new Error("APK contains duplicate or ambiguous native-library entries");
    }
  }
  return selected.sort();
}

export function verifyApk({ apk, readelf, jar }) {
  const apkPath = path.resolve(apk);
  if (!fs.statSync(apkPath).isFile()) throw new Error("APK is not a regular file");
  const commandOptions = {
    encoding: "utf8",
    timeout: 60_000,
    maxBuffer: 4 * 1024 * 1024,
    windowsHide: true,
  };
  const listing = execFileSync(jar, ["tf", apkPath], commandOptions);
  const libraries = selectArm64Libraries(listing);
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "landrop-elf-check-"));
  try {
    // Exact validated entries only, not arbitrary archive paths. Works with
    // the existing JDK17 on Linux runners and native Windows JDK installations.
    execFileSync(jar, ["xf", apkPath, ...libraries], { ...commandOptions, cwd: temporary });
    return libraries.map((entry) => {
      const library = path.join(temporary, ...entry.split("/"));
      const realLibrary = fs.realpathSync(library);
      if (
        !fs.lstatSync(library).isFile() ||
        !realLibrary.startsWith(fs.realpathSync(temporary) + path.sep)
      ) {
        throw new Error("Native library extraction escaped the owned temporary directory");
      }
      const output = execFileSync(readelf, ["-h", "-l", "-W", library], commandOptions);
      return { library: entry, ...verifyReadelf(output, entry) };
    });
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

function main(args) {
  const options = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i];
    if (!["--apk", "--readelf", "--jar"].includes(key) || !args[i + 1] || options[key.slice(2)]) {
      throw new Error(
        "Usage: node Verify-AndroidElfAlignment.mjs --apk FILE --readelf NDK_LLVM_READELF [--jar JDK_JAR]",
      );
    }
    options[key.slice(2)] = args[i + 1];
  }
  if (!options.apk || !options.readelf) throw new Error("--apk and --readelf are required");
  options.jar ??= process.env.JAVA_HOME
    ? path.join(process.env.JAVA_HOME, "bin", process.platform === "win32" ? "jar.exe" : "jar")
    : "jar";
  const results = verifyApk(options);
  for (const result of results) {
    console.log(
      `PASS ${result.library}: ${result.loadCount} LOAD segments >=16 KB; ${result.relroCount} RELRO segment ends aligned`,
    );
  }
  console.log(
    `Verified ELF 16 KB alignment for all ${results.length} packaged arm64 libraries. ZIP and device checks are separate.`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`Android ELF alignment gate failed: ${error.message}`);
    process.exitCode = 1;
  }
}
