import { invoke } from "@tauri-apps/api/core";
import type { HostKeyRejection } from "../types";

interface Pending {
  rejection: HostKeyRejection;
  resolve: (trusted: boolean) => void;
}

/** Trust prompt for unknown/changed host keys. One dialog at a time; parallel
 *  connects to the same host (e.g. readiness check + terminal) share one answer. */
class HostKeyPromptStore {
  current = $state<Pending | null>(null);
  busy = $state(false);
  error = $state<string | null>(null);

  private queue: Pending[] = [];
  private inflight = new Map<string, Promise<boolean>>();

  ask(rejection: HostKeyRejection): Promise<boolean> {
    const k = `${rejection.host}:${rejection.port}:${rejection.fingerprint}`;
    const existing = this.inflight.get(k);
    if (existing) return existing;
    const p = new Promise<boolean>((resolve) => {
      this.queue.push({ rejection, resolve });
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

  async trust() {
    const cur = this.current;
    if (!cur || this.busy) return;
    this.busy = true;
    try {
      const r = cur.rejection;
      await invoke("trust_host_key", {
        host: r.host,
        port: r.port,
        keyOpenssh: r.keyOpenssh,
      });
      this.finish(true);
    } catch (e) {
      this.error = String(e);
    } finally {
      this.busy = false;
    }
  }

  reject() {
    if (!this.busy) this.finish(false);
  }

  private finish(trusted: boolean) {
    const cur = this.current;
    this.current = null;
    cur?.resolve(trusted);
    this.next();
  }
}

export const hostKeyPrompt = new HostKeyPromptStore();
