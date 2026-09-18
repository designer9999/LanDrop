import assert from "node:assert/strict";
import test from "node:test";
import { selectArm64Libraries, verifyReadelf } from "./Verify-AndroidElfAlignment.mjs";

const valid = `ELF Header:
  Class:                             ELF64
  Type:                              DYN (Shared object file)
  Machine:                           AArch64
Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x004000 0x004000 R   0x4000
  LOAD           0x004000 0x0000000000004000 0x0000000000004000 0x004000 0x004000 R E 0x4000
  LOAD           0x008000 0x0000000000008000 0x0000000000008000 0x004000 0x004000 RW  0x4000
  GNU_RELRO      0x008000 0x0000000000008000 0x0000000000008000 0x004000 0x004000 R   0x1
`;

test("accepts 16 KB ELF, including CRLF output", () => {
  assert.deepEqual(verifyReadelf(valid), { loadCount: 3, relroCount: 1 });
  assert.deepEqual(verifyReadelf(valid.replaceAll("\n", "\r\n")), { loadCount: 3, relroCount: 1 });
});

test("accepts larger power-of-two LOAD alignment", () => {
  const larger =
    valid
      .split("\n")
      .filter((line) => !line.includes("0x004000") && !line.includes("GNU_RELRO"))
      .join("\n") +
    "\n  LOAD 0x000000 0x0000000000000000 0x0000000000000000 0x008000 0x008000 R E 0x10000\n";
  assert.equal(verifyReadelf(larger).loadCount, 1);
});

test("rejects 4 KB LOAD alignment even when ZIP might be aligned", () => {
  assert.throws(() => verifyReadelf(valid.replaceAll("0x4000", "0x1000")), /LOAD alignment/);
});

test("rejects unaligned RELRO independently of correct LOAD alignment", () => {
  const bad = valid.replace(
    /^ {2}GNU_RELRO.*$/m,
    "  GNU_RELRO 0x008000 0x0000000000008000 0x0000000000008000 0x004000 0x005000 R 0x1",
  );
  assert.throws(() => verifyReadelf(bad), /GNU_RELRO end/);
});

test("uses virtual address plus MemSiz, not file size or offset", () => {
  const changed = valid.replace(
    /^ {2}GNU_RELRO.*$/m,
    "  GNU_RELRO 0x007123 0x0000000000008100 0x0000000000008100 0x001234 0x003f00 R 0x1",
  );
  assert.equal(verifyReadelf(changed).relroCount, 1);
});

test("missing RELRO is page-compatible but reported as zero", () => {
  assert.equal(verifyReadelf(valid.replace(/^ {2}GNU_RELRO.*\n/m, "")).relroCount, 0);
});

test("rejects missing or malformed LOAD tables and wrong ELF architecture", () => {
  for (const invalid of [
    valid.replace(/^ {2}LOAD.*\n/gm, ""),
    valid.replace("LOAD           0x000000", "LOAD broken"),
    valid.replace("ELF64", "ELF32"),
    valid.replace("AArch64", "X86-64"),
    valid.replace("DYN (Shared object file)", "EXEC (Executable file)"),
    valid.replaceAll("0x4000", "0x5000"),
    valid.replace("LOAD           0x004000", "LOAD           0x004001"),
  ])
    assert.throws(() => verifyReadelf(invalid));
});

test("checks every packaged arm64 library", () => {
  assert.deepEqual(
    selectArm64Libraries("classes.dex\nlib/arm64-v8a/libz.so\nlib/arm64-v8a/liba.so\n"),
    ["lib/arm64-v8a/liba.so", "lib/arm64-v8a/libz.so"],
  );
});

test("rejects wrong ABI, missing libraries, duplicates and path traversal", () => {
  for (const invalid of [
    "classes.dex\n",
    "lib/x86_64/liba.so\n",
    "lib/arm64-v8a/../liba.so\n",
    "lib/arm64-v8a/liba.so\nlib/arm64-v8a/liba.so\n",
    "lib/arm64-v8a/liba.so\nlib/arm64-v8a/liba.so/file\n",
  ])
    assert.throws(() => selectArm64Libraries(invalid));
});
