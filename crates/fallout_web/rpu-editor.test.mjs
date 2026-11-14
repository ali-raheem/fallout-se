import { readFile } from "node:fs/promises";
import test from "node:test";
import assert from "node:assert/strict";
const source = await readFile(new URL("./rpu-editor.js", import.meta.url), "utf8");
const { strictQuantity, slotFiles, verifyDownloadedBytes } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);

test("quantity text is strictly validated before WASM integer conversion", () => {
  for (const invalid of ["", "0", "-1", "1.5", "1e4", " 12", "12 ", "Infinity", "2147483648"]) {
    assert.throws(() => strictQuantity(invalid));
  }
  for (const valid of ["1", "10001", "2147483647"]) assert.equal(strictQuantity(valid), Number(valid));
});

test("complete folder paths are required and case collisions are ambiguous", () => {
  const files = ["SAVE.DAT", "sfallfs.sav", "AUTOMAP.SAV", "sfalldb.sav", "sfallgv.sav"]
    .map(name => ({ webkitRelativePath: `SLOT03/${name}` }));
  assert.equal(slotFiles(files).size, 5);
  assert.throws(() => slotFiles(files.slice(1)), /save.dat/);
  assert.throws(() => slotFiles([...files, { webkitRelativePath: "SLOT03/save.dat" }]), /Ambiguous/);
  assert.throws(() => slotFiles([...files, { webkitRelativePath: "SLOT04/other" }]), /exactly one/);
  assert.throws(() => slotFiles([{ webkitRelativePath: "SAVE.DAT" }]), /folder/);
});

test("independent browser byte comparison rejects unrelated mutations and false receipts", () => {
  const original = new Uint8Array(12);
  const output = new Uint8Array(original);
  new DataView(output.buffer).setInt32(4,10001,false);
  const receipt = { verified: true, quantity_span: [4,8], changed_offsets: [6,7] };
  verifyDownloadedBytes(original,output,receipt,10001);
  assert.throws(() => verifyDownloadedBytes(original,output,receipt,10002), /encoding/);
  assert.throws(() => verifyDownloadedBytes(original,output,{...receipt, changed_offsets:[]},10001), /receipt/);
  const corrupted = new Uint8Array(output); corrupted[11] = 1;
  assert.throws(() => verifyDownloadedBytes(original,corrupted,receipt,10001), /unrelated/);
  assert.throws(() => verifyDownloadedBytes(original,output.subarray(1),receipt,10001), /shape/);
});
