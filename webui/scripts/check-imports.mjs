import { readdir, readFile, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join, relative } from "node:path";

const root = fileURLToPath(new URL("../src/", import.meta.url));
const forbidden = [
  [/\bapp\.use\s*\(\s*TDesign\b/, "global TDesign registration"],
  [/(?:from\s+["']tdesign-mobile-vue["']|import\s*["']tdesign-mobile-vue["'])/, "TDesign import"],
  [/import\s+\*\s+as\s+\w+\s+from\s+["']@tabler\/icons-vue["']/, "Tabler namespace import"],
  [/from\s+["'](?:pinia|axios|unplugin-vue-components)["']/, "forbidden runtime dependency"],
];
const kernelsuImports = /from\s+["']kernelsu["']/g;

async function sourceFiles(directory) {
  if ((await stat(directory)).isFile()) return [directory];
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await sourceFiles(path));
    else if (/\.(?:ts|vue)$/.test(entry.name)) files.push(path);
  }
  return files;
}

const targets = process.argv.length > 2 ? process.argv.slice(2) : [root];
for (const target of targets) {
for (const path of await sourceFiles(target)) {
  const source = await readFile(path, "utf8");
  for (const [pattern, label] of forbidden) {
    if (pattern.test(source)) throw new Error(`${label}: ${relative(root, path)}`);
  }
  const imports = [...source.matchAll(kernelsuImports)];
  if (imports.length > 0 && relative(root, path).replaceAll("\\", "/") !== "bridge/kernelsu-host.ts") {
    throw new Error(`KernelSU import must stay in bridge/kernelsu-host.ts: ${relative(root, path)}`);
  }
}
}
console.log("WebUI import contracts passed");
