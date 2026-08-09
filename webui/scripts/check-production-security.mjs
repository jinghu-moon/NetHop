import { readdir, readFile } from "node:fs/promises";
import { join, resolve } from "node:path";

const webroot = resolve("../module/webroot");
if (process.argv[2] === "--scan-fixture") {
  const source = await readFile(process.argv[3], "utf8");
  if (/https?:\/\//i.test(source) || /connect-src\s+(?!'none')/i.test(source)) {
    throw new Error("fixture contains a remote network capability");
  }
  console.log("WebUI security fixture passed");
  process.exit(0);
}
const index = await readFile(join(webroot, "index.html"), "utf8");
const required = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self'",
  "connect-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
];
for (const directive of required) {
  if (!index.includes(directive)) throw new Error(`production CSP missing: ${directive}`);
}
if (/https?:\/\//i.test(index)) throw new Error("production index contains a remote URL");

async function files(directory) {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...await files(path));
    else result.push(path);
  }
  return result;
}
for (const path of await files(webroot)) {
  if (!/\.(?:html|js|css|json)$/.test(path)) continue;
  const source = await readFile(path, "utf8");
  const withoutInertNamespaces = source
    .replaceAll("http://www.w3.org/1998/Math/MathML", "")
    .replaceAll("http://www.w3.org/1999/xlink", "")
    .replaceAll("http://www.w3.org/2000/svg", "")
    .replaceAll("https://vuejs.org/error-reference/#runtime-${r}", "");
  if (/https?:\/\//i.test(withoutInertNamespaces)) throw new Error(`remote URL found in ${path}`);
  if (/sourceMappingURL=/.test(source)) throw new Error(`source map reference found in ${path}`);
}
console.log("WebUI production security contracts passed");
