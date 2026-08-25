import { spawn } from "node:child_process";
import net from "node:net";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const viteEntry = join(dirname(require.resolve("vite/package.json")), "bin", "vite.js");
const userArgs = process.argv.slice(2);
const hasOpenArgument = userArgs.some((argument) => argument === "--open" || argument.startsWith("--open="));
const openArgs = hasOpenArgument ? [] : ["--open", "/#/dev/ui-foundation"];
const portArgument = userArgs.find((argument) => argument.startsWith("--port="));
const portIndex = userArgs.indexOf("--port");
const requestedPort = Number(portArgument?.slice("--port=".length) ?? (portIndex >= 0 ? userArgs[portIndex + 1] : "5173"));

function isPortAvailable(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => server.close(() => resolve(true)));
    server.listen(port, "127.0.0.1");
  });
}

const displayPort = await (async () => {
  for (let port = requestedPort; port < requestedPort + 20; port += 1) {
    if (await isPortAvailable(port)) return port;
  }
  throw new Error(`No available port in ${requestedPort}-${requestedPort + 19}`);
})();
const vitePortArgs = ["--port", String(displayPort), "--strictPort"];
const child = spawn(process.execPath, [viteEntry, "--host", "127.0.0.1", ...vitePortArgs, ...openArgs, ...userArgs.filter((argument, index) => argument !== "--port" && index !== portIndex + 1 && !argument.startsWith("--port="))], {
  env: { ...process.env, VITE_ENABLE_UI_FOUNDATION: "true" },
  stdio: "inherit",
});

process.stdout.write(`UI Foundation: http://127.0.0.1:${displayPort}/#/dev/ui-foundation\n`);

child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
