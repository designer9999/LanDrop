import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const resources = new URL("../src-tauri/gen/android/app/src/main/res/", import.meta.url);
const densities = { mdpi: 1, hdpi: 1.5, xhdpi: 2, xxhdpi: 3, xxxhdpi: 4 };
const icons = {
  "ic_launcher.png": 48,
  "ic_launcher_round.png": 48,
  "ic_launcher_foreground.png": 108,
};

test("all Android launcher density variants have their required pixel dimensions", () => {
  for (const [density, scale] of Object.entries(densities)) {
    for (const [name, dp] of Object.entries(icons)) {
      const label = `mipmap-${density}/${name}`;
      const data = fs.readFileSync(new URL(label, resources));
      assert(data.length >= 24, `${label}: truncated PNG`);
      assert.equal(data.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", `${label}: not PNG`);
      assert.equal(data.toString("ascii", 12, 16), "IHDR", `${label}: missing PNG header`);
      assert.equal(data.readUInt32BE(16), dp * scale, `${label}: incorrect width`);
      assert.equal(data.readUInt32BE(20), dp * scale, `${label}: incorrect height`);
    }
  }
});
