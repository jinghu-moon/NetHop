import { gzipSync } from "node:zlib";
import { readdir, readFile, stat } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const webroot = resolve("../module/webroot");
const budget = JSON.parse(await readFile(new URL("../webui-budget.json", import.meta.url), "utf8"));
if (process.argv[2] === "--probe-gzip") {
  const bytes = Number(process.argv[3]);
  if (!Number.isSafeInteger(bytes) || bytes < 0) throw new Error("invalid bundle budget probe");
  if (bytes > budget.asyncChunkGzipBytes) throw new Error("bundle budget probe exceeded");
  console.log("WebUI bundle budget probe passed");
  process.exit(0);
}
const index = await readFile(join(webroot, "index.html"), "utf8");
const entryScripts = new Set([...index.matchAll(/<script[^>]+src="\.\/([^"]+\.js)"/g)].map((match) => basename(match[1])));
const entryStyles = new Set([...index.matchAll(/<link[^>]+href="\.\/([^"]+\.css)"/g)].map((match) => basename(match[1])));

async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const result = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...await files(path));
    else result.push(path);
  }
  return result;
}

const assets = await files(webroot);
let total = 0;
const records = [];
for (const path of assets) {
  const bytes = (await stat(path)).size;
  total += bytes;
  if (!/\.(?:js|css)$/.test(path)) continue;
  const gzip = gzipSync(await readFile(path), { level: 9 }).length;
  const name = basename(path);
  const limit = path.endsWith(".css")
    ? (entryStyles.has(name) ? budget.entryCssGzipBytes : budget.asyncChunkGzipBytes)
    : (entryScripts.has(name) ? budget.entryJavaScriptGzipBytes : budget.asyncChunkGzipBytes);
  records.push({ name, gzip, limit });
  if (gzip > limit) throw new Error(`bundle budget exceeded: ${name} ${gzip} > ${limit}`);
}
if (total > budget.webrootBytes) throw new Error(`webroot budget exceeded: ${total} > ${budget.webrootBytes}`);
records.sort((left, right) => right.gzip - left.gzip);
console.table(records.slice(0, 20));
console.log(`WebUI bundle budget passed: ${total} bytes total`);
