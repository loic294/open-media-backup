import type { ReactiveController, ReactiveControllerHost } from "lit";
import type { AppStore } from "./app-store";

/** Re-renders a Lit host whenever the store changes. */
export class StoreController implements ReactiveController {
  #onChange = () => this.host.requestUpdate();

  constructor(
    private host: ReactiveControllerHost,
    readonly store: AppStore,
  ) {
    host.addController(this);
  }

  hostConnected(): void {
    this.store.addEventListener("change", this.#onChange);
  }

  hostDisconnected(): void {
    this.store.removeEventListener("change", this.#onChange);
  }
}
