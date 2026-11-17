import { invoke } from "@tauri-apps/api/core";
import type { SecretRequest } from "../types";

interface Pending {
  request: SecretRequest;
  resolve: (provided: boolean) => void;
}

/** Prompt for passwords (server/jump host) and key passphrases the backend
 *  asked for. Parallel requests for the same secret share one dialog. */
class SecretPromptStore {
  current = $state<Pending | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);

  private queue: Pending[] = [];
  private inflight = new Map<string, Promise<boolean>>();

  ask(request: SecretRequest): Promise<boolean> {
    const k = `${request.kind}:${request.id}`;
    const existing = this.inflight.get(k);
    if (existing) return existing;
    const p = new Promise<boolean>((resolve) => {
      this.queue.push({ request, resolve });
      this.next();
    });
    this.inflight.set(k, p);
    p.finally(() => this.inflight.delete(k));
    return p;
  }

  private next() {
    if (!this.current) {
      this.current = this.queue.shift() ?? null;
      this.error = null;
    }
  }

  async submit(value: string, remember: boolean) {
    const cur = this.current;
    if (!cur || this.busy || !value) return;
    this.busy = true;
    this.error = null;
    try {
      await invoke("provide_secret", {
        kind: cur.request.kind,
        id: cur.request.id,
        value,
        remember,
      });
      this.finish(true);
    } catch (e) {
      // Wrong passphrase is detected locally; keep the dialog open.
      const msg = e && typeof e === "object" && "message" in e ? (e as { message: string }).message : String(e);
      this.error = msg;
    } finally {
      this.busy = false;
    }
  }

  cancel() {
    if (!this.busy) this.finish(false);
  }

  private finish(provided: boolean) {
    const cur = this.current;
    this.current = null;
    cur?.resolve(provided);
    this.next();
  }
}

export const secretPrompt = new SecretPromptStore();
