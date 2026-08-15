import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { basename, join, relative, resolve } from "node:path";

const projectRoot = resolve(new URL("..", import.meta.url).pathname.replace(/^\/(?:[A-Za-z]:)/, (value) => value.slice(1)));
const workspace = resolve(projectRoot, "..");
const webroot = resolve(workspace, "module/webroot");
const artifactRoot = resolve(workspace, "artifacts/webui");
const lock = JSON.parse(await readFile(join(projectRoot, "package-lock.json"), "utf8"));
const packageJson = JSON.parse(await readFile(join(projectRoot, "package.json"), "utf8"));
const metafile = JSON.parse(await readFile(join(artifactRoot, "bundle-metafile.json"), "utf8"));

async function walk(directory) {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...await walk(path));
    else result.push(path);
  }
  return result;
}

function sha256(content) {
  return createHash("sha256").update(content).digest("hex");
}

function packageName(path) {
  return path.split("node_modules/").at(-1);
}

function purl(name, version) {
  const encoded = name.split("/").map(encodeURIComponent).join("/");
  return `pkg:npm/${encoded}@${version}`;
}

await mkdir(artifactRoot, { recursive: true });
const index = await readFile(join(webroot, "index.html"), "utf8");
const entryScripts = new Set([...index.matchAll(/<script[^>]+src="\.\/([^"]+\.js)"/g)].map((match) => basename(match[1])));
const entryStyles = new Set([...index.matchAll(/<link[^>]+href="\.\/([^"]+\.css)"/g)].map((match) => basename(match[1])));
const assets = [];
for (const path of await walk(webroot)) {
  const content = await readFile(path);
  const name = relative(webroot, path).replaceAll("\\", "/");
  assets.push({
    path: name,
    bytes: (await stat(path)).size,
    sha256: sha256(content),
    ...(name.endsWith(".js") || name.endsWith(".css") ? { gzip_bytes: gzipSync(content, { level: 9 }).length } : {}),
  });
}
assets.sort((left, right) => left.path.localeCompare(right.path));

const moduleBytes = new Map();
for (const chunk of metafile.chunks) {
  for (const module of chunk.modules) moduleBytes.set(module.id, (moduleBytes.get(module.id) ?? 0) + module.rendered_bytes);
}
const topModules = [...moduleBytes.entries()]
  .map(([id, rendered_bytes]) => ({ id, rendered_bytes }))
  .sort((left, right) => right.rendered_bytes - left.rendered_bytes)
  .slice(0, 20);
const bundleReport = {
  schema: "nethop.webui.production-bundle.v1",
  package: { name: packageJson.name, version: packageJson.version },
  totals: {
    webroot_bytes: assets.reduce((sum, asset) => sum + asset.bytes, 0),
    javascript_gzip_bytes: assets.filter((asset) => asset.path.endsWith(".js")).reduce((sum, asset) => sum + asset.gzip_bytes, 0),
    css_gzip_bytes: assets.filter((asset) => asset.path.endsWith(".css")).reduce((sum, asset) => sum + asset.gzip_bytes, 0),
  },
  entry: {
    scripts: assets.filter((asset) => entryScripts.has(basename(asset.path))),
    styles: assets.filter((asset) => entryStyles.has(basename(asset.path))),
  },
  top_modules: topModules,
  assets,
};
const bundlePath = join(artifactRoot, "production-bundle.json");
await writeFile(bundlePath, `${JSON.stringify(bundleReport, null, 2)}\n`);

const direct = new Set([...Object.keys(packageJson.dependencies), ...Object.keys(packageJson.devDependencies)]);
const packageComponents = Object.entries(lock.packages)
  .filter(([path, metadata]) => path.startsWith("node_modules/") && !metadata.link && metadata.version)
  .map(([path, metadata]) => {
    const name = packageName(path);
    return {
      type: "library",
      "bom-ref": `npm:${path}`,
      name,
      version: metadata.version,
      scope: Object.hasOwn(packageJson.dependencies, name) ? "required" : "optional",
      purl: purl(name, metadata.version),
      licenses: metadata.license ? [{ license: { id: metadata.license } }] : [],
      properties: [{ name: "nethop:direct", value: String(direct.has(name)) }],
    };
  })
  .sort((left, right) => left["bom-ref"].localeCompare(right["bom-ref"]));
const vendoredComponents = [
  {
    type: "data",
    "bom-ref": "vendored:unicode-cldr:48.2.0",
    name: "Unicode CLDR territory data",
    version: "48.2.0",
    scope: "required",
    purl: "pkg:generic/unicode-cldr@48.2.0",
    licenses: [{ license: { id: "Unicode-3.0" } }],
    properties: [
      { name: "nethop:direct", value: "true" },
      { name: "nethop:vendored", value: "true" },
    ],
  },
  {
    type: "library",
    "bom-ref": "vendored:country-flag-icons:1.6.20",
    name: "country-flag-icons",
    version: "1.6.20",
    scope: "required",
    purl: "pkg:npm/country-flag-icons@1.6.20",
    licenses: [{ license: { id: "MIT" } }],
    properties: [
      { name: "nethop:direct", value: "true" },
      { name: "nethop:vendored", value: "true" },
    ],
  },
];
const components = [...packageComponents, ...vendoredComponents]
  .sort((left, right) => left["bom-ref"].localeCompare(right["bom-ref"]));
const sbom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  serialNumber: `urn:uuid:${sha256(await readFile(join(projectRoot, "package-lock.json"))).slice(0, 8)}-${sha256(packageJson.name).slice(0, 4)}-4${sha256(packageJson.version).slice(0, 3)}-8${sha256(JSON.stringify(components)).slice(0, 3)}-${sha256(JSON.stringify(lock.packages)).slice(0, 12)}`,
  version: 1,
  metadata: { component: { type: "application", name: packageJson.name, version: packageJson.version } },
  components,
};
const sbomPath = join(artifactRoot, "webui-sbom.cdx.json");
await writeFile(sbomPath, `${JSON.stringify(sbom, null, 2)}\n`);

const licenses = {
  schema: "nethop.webui.licenses.v1",
  packages: components.map((component) => ({
    name: component.name,
    version: component.version,
    direct: component.properties.some((property) => property.name === "nethop:direct" && property.value === "true"),
    vendored: component.properties.some((property) => property.name === "nethop:vendored" && property.value === "true"),
    licenses: component.licenses.map((entry) => entry.license.id),
    purl: component.purl,
  })),
};
const licensesPath = join(artifactRoot, "webui-licenses.json");
await writeFile(licensesPath, `${JSON.stringify(licenses, null, 2)}\n`);

const checksumPaths = [bundlePath, join(artifactRoot, "bundle-metafile.json"), sbomPath, licensesPath];
const checksums = [];
for (const path of checksumPaths) checksums.push(`${sha256(await readFile(path))}  ${basename(path)}`);
await writeFile(join(artifactRoot, "checksums.sha256"), `${checksums.join("\n")}\n`, "ascii");

console.log(`WebUI release artifacts generated: ${assets.length} assets, ${components.length} packages`);
