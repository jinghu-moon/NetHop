import type { HostAdapter } from "@/bridge/host";
import { runJson } from "@/bridge/command";
import type { OperationRequest } from "@/bridge/operations";

import { parseControlEnvelope } from "./dto";

export async function validatedQuery<T>(host: HostAdapter, request: OperationRequest, validator: (value: unknown) => T): Promise<T> {
  const result = await runJson(host, request);
  return parseControlEnvelope(result.response, validator).result;
}
