import { enableEdgeToEdge, exec, exit, getPackagesInfo, listPackages, spawn, toast } from "kernelsu";

import type { HostAdapter, HostCapability, HostChild, PackageInfo } from "./host";
import { buildCommand, type OperationRequest } from "./operations";

function commandLine(request: OperationRequest): string {
  const command = buildCommand(request);
  return [command.executable, ...command.args].map((part) => `"${part.replaceAll('"', '\\"')}"`).join(" ");
}

function wrapChild(child: ReturnType<typeof spawn>, request: OperationRequest): HostChild {
  const stdoutListeners = new Set<(chunk: string) => void>();
  const stderrListeners = new Set<(chunk: string) => void>();
  const exitListeners = new Set<(code: number | null) => void>();
  const errorListeners = new Set<(error: unknown) => void>();
  child.stdout.on("data", (chunk: string) => stdoutListeners.forEach((listener) => listener(chunk)));
  child.stderr.on("data", (chunk: string) => stderrListeners.forEach((listener) => listener(chunk)));
  child.on("exit", (code: number) => exitListeners.forEach((listener) => listener(code)));
  child.on("error", (error: unknown) => errorListeners.forEach((listener) => listener(error)));
  let terminated = false;
  return {
    stdout: { onData(listener) { stdoutListeners.add(listener); return () => stdoutListeners.delete(listener); } },
    stderr: { onData(listener) { stderrListeners.add(listener); return () => stderrListeners.delete(listener); } },
    onExit(listener) { exitListeners.add(listener); return () => exitListeners.delete(listener); },
    onError(listener) { errorListeners.add(listener); return () => errorListeners.delete(listener); },
    terminate() {
      if (terminated) return;
      terminated = true;
      const kill = (child as unknown as { kill?: () => void }).kill;
      if (typeof kill === "function") kill.call(child);
      if (request.id === "events.subscribe") {
        void exec(commandLine({ id: "events.terminate", sessionId: request.sessionId }));
      }
    },
  };
}

export function createKernelSuHost(): HostAdapter {
  const capability: HostCapability = {
    kind: "kernelsu",
    available: true,
    methods: ["exec", "spawn", "toast", "listPackages", "getPackagesInfo", "enableEdgeToEdge", "exit"],
  };
  return {
    capability,
    run(request) {
      return exec(commandLine(request));
    },
    spawn(request) {
      const command = buildCommand(request);
      return wrapChild(spawn(command.executable, [...command.args]), request);
    },
    listPackages(type) { return listPackages(type) as readonly string[]; },
    getPackagesInfo(packages) { return getPackagesInfo([...packages]) as readonly PackageInfo[]; },
    toast,
    enableEdgeToEdge,
    exit,
  };
}
