type Handler = (payload: never) => void;

export class Emitter {
  #handlers = new Map<string, Set<Handler>>();

  on(event: string, handler: Handler): () => void {
    const set = this.#handlers.get(event) ?? new Set();
    set.add(handler);
    this.#handlers.set(event, set);
    return () => set.delete(handler);
  }

  emit(event: string, payload?: unknown): void {
    this.#handlers.get(event)?.forEach((h) => h(payload as never));
  }
}
