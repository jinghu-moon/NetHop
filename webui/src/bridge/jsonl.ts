import { parseSingleJsonEnvelope } from "./json";

export const MAX_JSONL_LINE_BYTES = 16 * 1024;

export class JsonlDecoder {
  private buffer = "";
  private disposed = false;
  push(chunk: string): unknown[] {
    if (this.disposed) return [];
    this.buffer += chunk;
    if (new TextEncoder().encode(this.buffer).byteLength > MAX_JSONL_LINE_BYTES * 2) throw new Error("JSONL buffer exceeds bound");
    const frames: unknown[] = [];
    let newline = this.buffer.indexOf("\n");
    while (newline >= 0) {
      let line = this.buffer.slice(0, newline);
      this.buffer = this.buffer.slice(newline + 1);
      if (line.endsWith("\r")) line = line.slice(0, -1);
      if (line.length > 0) {
        if (new TextEncoder().encode(line).byteLength > MAX_JSONL_LINE_BYTES) throw new Error("JSONL line exceeds bound");
        frames.push(parseSingleJsonEnvelope(line));
      }
      newline = this.buffer.indexOf("\n");
    }
    return frames;
  }
  finish(): void {
    if (this.disposed) return;
    if (this.buffer.trim() !== "") throw new Error("truncated JSONL frame");
    this.dispose();
  }
  dispose(): void { this.buffer = ""; this.disposed = true; }
}
