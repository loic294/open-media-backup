import { backend } from "../api";
import { AppStore } from "./app-store";

/** The app-wide store instance. Components read it through OmbElement. */
export const store = new AppStore(backend);

export { AppStore } from "./app-store";
export type { DialogRequest } from "./dialogs";
