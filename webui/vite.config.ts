import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath, URL } from "node:url";

import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

const productionCsp = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self'",
  "img-src 'self' data:",
  "font-src 'self'",
  "connect-src 'none'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
].join("; ");

const projectRoot = fileURLToPath(new URL(".", import.meta.url));
const bundleMetafile = resolve(projectRoot, "../artifacts/webui/bundle-metafile.json");

function normalizedModuleId(id: string): string {
  return relative(projectRoot, id).replaceAll("\\", "/");
}

export default defineConfig(({ command }) => ({
  base: "./",
  plugins: [
    vue(),
    {
      name: "nethop-production-csp",
      transformIndexHtml: command === "build"
        ? {
            order: "pre",
            handler: (html) => html.replace(
              "<meta charset=\"UTF-8\" />",
              `<meta charset="UTF-8" />\n    <meta http-equiv="Content-Security-Policy" content="${productionCsp}" />`,
            ),
          }
        : undefined,
    },
    {
      name: "nethop-bundle-metafile",
      generateBundle(_options, bundle) {
        if (command !== "build") return;
        const chunks = Object.values(bundle)
          .filter((item) => item.type === "chunk")
          .map((chunk) => ({
            file: chunk.fileName,
            entry: chunk.isEntry,
            dynamic_entry: chunk.isDynamicEntry,
            imports: [...chunk.imports].sort(),
            dynamic_imports: [...chunk.dynamicImports].sort(),
            modules: Object.entries(chunk.modules)
              .map(([id, metadata]) => ({ id: normalizedModuleId(id), rendered_bytes: metadata.renderedLength }))
              .sort((left, right) => right.rendered_bytes - left.rendered_bytes),
          }))
          .sort((left, right) => left.file.localeCompare(right.file));
        mkdirSync(dirname(bundleMetafile), { recursive: true });
        writeFileSync(bundleMetafile, `${JSON.stringify({ schema: "nethop.webui.bundle-metafile.v1", chunks }, null, 2)}\n`);
      },
    },
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  build: {
    target: "chrome105",
    outDir: "../module/webroot",
    emptyOutDir: true,
    sourcemap: false,
    manifest: true,
    cssCodeSplit: true,
    reportCompressedSize: true,
  },
}));
