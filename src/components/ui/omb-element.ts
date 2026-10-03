import { LitElement } from "lit";
import { store } from "../../state";
import { StoreController } from "../../state/controller";

/** Light-DOM Lit element so Tailwind/daisyUI classes apply. Re-renders on store changes. */
export class OmbElement extends LitElement {
  protected readonly store = store;

  constructor() {
    super();
    new StoreController(this, store);
  }

  protected override createRenderRoot(): HTMLElement {
    return this;
  }
}

/** Light-DOM element that does not subscribe to the store (pure presentational). */
export class OmbPureElement extends LitElement {
  protected override createRenderRoot(): HTMLElement {
    return this;
  }
}
