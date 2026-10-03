import { query } from "lit/decorators.js";
import { property } from "lit/decorators.js";
import type { DialogRequest } from "../../state/dialogs";
import { OmbElement } from "../ui/omb-element";
import type { OmbModal } from "../ui/omb-modal";
import "../ui/omb-modal";

/** Base for dialogs: holds the request and removes it from the store when the modal closes. */
export class DialogBase<R extends DialogRequest> extends OmbElement {
  @property({ attribute: false }) request!: R;
  @query("omb-modal") protected modal!: OmbModal;

  /** Close with animation; the modal's "close" event then calls onClosed(). */
  protected dismiss(): void {
    this.modal?.close();
  }

  protected onClosed = (): void => {
    this.store.close(this.request);
  };
}
