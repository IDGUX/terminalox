export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

class ToastStore {
  items = $state<Toast[]>([]);
  private next = 1;

  push(kind: ToastKind, message: string, ttlMs = 3000) {
    const id = this.next++;
    this.items = [...this.items, { id, kind, message }];
    setTimeout(() => this.dismiss(id), ttlMs);
  }

  dismiss(id: number) {
    this.items = this.items.filter((t) => t.id !== id);
  }

  info(msg: string, ttlMs?: number) {
    this.push("info", msg, ttlMs);
  }
  success(msg: string, ttlMs?: number) {
    this.push("success", msg, ttlMs);
  }
  error(msg: string, ttlMs = 6000) {
    this.push("error", msg, ttlMs);
  }
}

export const toast = new ToastStore();
