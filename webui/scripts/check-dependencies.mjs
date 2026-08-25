import { readFile, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";

function assertSourceBoundary(source, label) {
  const forbidden = [
    [/\bfetch\s*\(/, "fetch"],
    [/\bWebSocket\s*\(/, "WebSocket"],
    [/\bsetInterval\s*\(/, "setInterval"],
  ];
  for (const [pattern, name] of forbidden) {
    if (pattern.test(source)) throw new Error(`${label} uses forbidden ${name}`);
  }
}

if (process.argv[2] === "--scan-fixture") {
  assertSourceBoundary(await readFile(process.argv[3], "utf8"), process.argv[3]);
  console.log("WebUI dependency fixture passed");
  process.exit(0);
}

const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
const lock = JSON.parse(await readFile(new URL("../package-lock.json", import.meta.url), "utf8"));

for (const [scope, dependencies] of Object.entries({
  dependencies: packageJson.dependencies,
  devDependencies: packageJson.devDependencies,
})) {
  for (const [name, version] of Object.entries(dependencies)) {
    if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
      throw new Error(`${scope} must use an exact version: ${name}@${version}`);
    }
  }
}
for (const [path, metadata] of Object.entries(lock.packages)) {
  if (!path.startsWith("node_modules/") || metadata.link || metadata.license) continue;
  try {
    const installed = JSON.parse(await readFile(resolve(path, "package.json"), "utf8"));
    if (installed.license) continue;
  } catch {
    // Fall through to the actionable error below when the installed package is unavailable.
  }
  throw new Error(`dependency license is missing from lockfile and installed metadata: ${path}`);
}

const exact = {
  "@vueuse/core": "14.4.0",
  "@tanstack/vue-virtual": "3.13.36",
  "@tabler/icons-vue": "3.46.0",
};
for (const [name, version] of Object.entries(exact)) {
  if (packageJson.dependencies[name] !== version) throw new Error(`${name} direct version drifted`);
  if (lock.packages[`node_modules/${name}`]?.version !== version) throw new Error(`${name} lock version drifted`);
}
if (lock.packages["node_modules/@tanstack/virtual-core"]?.version !== "3.17.8") {
  throw new Error("TanStack virtual-core version drifted");
}
const vueUseCopies = Object.entries(lock.packages)
  .filter(([path, value]) => path.endsWith("node_modules/@vueuse/core") && value.version)
  .map(([path, value]) => `${path}@${value.version}`);
if (vueUseCopies.length !== 1 || !vueUseCopies[0].endsWith("@14.4.0")) {
  throw new Error(`duplicate or incompatible VueUse runtimes: ${vueUseCopies.join(", ")}`);
}
if (packageJson.overrides?.["@vueuse/core"]) throw new Error("VueUse override is forbidden");
for (const forbidden of ["pinia", "axios", "vue-virtual-scroller", "@vueuse/components"]) {
  if (packageJson.dependencies[forbidden] || packageJson.devDependencies[forbidden]) {
    throw new Error(`forbidden dependency: ${forbidden}`);
  }
}
for (const [path] of Object.entries(lock.packages)) {
  const name = path.split("node_modules/").at(-1);
  if (name?.startsWith("@tanstack/") && !["@tanstack/vue-virtual", "@tanstack/virtual-core"].includes(name)) {
    throw new Error(`forbidden TanStack package: ${name}`);
  }
}

async function sourceFiles(directory) {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...await sourceFiles(path));
    else if (/\.(?:ts|vue)$/.test(entry.name)) result.push(path);
  }
  return result;
}
for (const path of await sourceFiles(resolve("src"))) assertSourceBoundary(await readFile(path, "utf8"), path);
console.log("WebUI dependency contracts passed");
