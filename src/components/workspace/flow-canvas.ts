import { html, nothing, svg } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { Flow, FlowState } from "../../api/types";
import { failureLabel, flowStatus } from "../../state/derived";
import { newFlow } from "../../state/factories";
import { spaceFlows, spaceSources } from "../../state/selectors";
import { formatCount } from "../../utils/format";
import { FLOW_COLOR } from "../ui/flow-colors";
import { OmbElement } from "../ui/omb-element";
import { curve, entryPoints, pointOnCurve, type Box, type Point } from "./flow-geometry";

interface Layout {
  ports: Record<string, Point>;
  boxes: Record<string, Box>;
}

interface Line {
  flow: Flow;
  from: Point;
  to: Point;
  state: FlowState;
}

/** SVG overlay drawing source → destination connections. Drag from a port to connect. */
@customElement("omb-flow-canvas")
export class OmbFlowCanvas extends OmbElement {
  @state() private layout: Layout = { ports: {}, boxes: {} };
  @state() private drag: { sourceId: string; from: Point; to: Point } | null = null;
  @state() private menu: { flow: Flow; at: Point } | null = null;
  #observer = new ResizeObserver(() => this.#measure());
  #frame = 0;

  get #board(): HTMLElement {
    return this.parentElement as HTMLElement;
  }

  override connectedCallback(): void {
    super.connectedCallback();
    queueMicrotask(() => {
      this.#observer.observe(this.#board);
      this.#board.addEventListener("pointerdown", this.#onPointerDown);
    });
    window.addEventListener("pointerdown", this.#closeMenu);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this.#observer.disconnect();
    this.#board?.removeEventListener("pointerdown", this.#onPointerDown);
    window.removeEventListener("pointerdown", this.#closeMenu);
  }

  override updated(): void {
    cancelAnimationFrame(this.#frame);
    this.#frame = requestAnimationFrame(() => this.#measure());
  }

  #relative(rect: DOMRect, origin: DOMRect) {
    return { top: rect.top - origin.top, left: rect.left - origin.left, right: rect.right - origin.left, height: rect.height };
  }

  #measure() {
    const origin = this.#board.getBoundingClientRect();
    const ports: Layout["ports"] = {};
    const boxes: Layout["boxes"] = {};
    this.#board.querySelectorAll<HTMLElement>("[data-port-source]").forEach((el) => {
      const r = this.#relative(el.getBoundingClientRect(), origin);
      ports[el.dataset.portSource!] = { x: (r.left + r.right) / 2, y: r.top + r.height / 2 };
    });
    this.#board.querySelectorAll<HTMLElement>("[data-dest-id]").forEach((el) => {
      boxes[el.dataset.destId!] = this.#relative(el.getBoundingClientRect(), origin);
    });
    const next = { ports, boxes };
    if (JSON.stringify(next) !== JSON.stringify(this.layout)) this.layout = next;
  }

  #lines(): Line[] {
    const { snapshot, space, status } = this.store;
    if (!snapshot || !space) return [];
    const order = new Map(spaceSources(snapshot, space.id).map((s, i) => [s.id, i]));
    const flows = spaceFlows(snapshot, space.id).sort((a, b) => (order.get(a.source_id) ?? 0) - (order.get(b.source_id) ?? 0));
    const lines: Line[] = [];
    const byDest = new Map<string, Flow[]>();
    flows.forEach((f) => byDest.set(f.destination_id, [...(byDest.get(f.destination_id) ?? []), f]));
    for (const [destId, incoming] of byDest) {
      const box = this.layout.boxes[destId];
      if (!box) continue;
      const ends = entryPoints(box, incoming.length);
      incoming.forEach((flow, i) => {
        const from = this.layout.ports[flow.source_id];
        if (from) lines.push({ flow, from, to: ends[i], state: flowStatus(status, flow.id)?.state ?? "empty" });
      });
    }
    return lines;
  }

  // ---- drag to connect ----

  #point(e: PointerEvent): Point {
    const origin = this.#board.getBoundingClientRect();
    return { x: e.clientX - origin.left, y: e.clientY - origin.top };
  }

  #dropTarget(e: PointerEvent): HTMLElement | null {
    return (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>("[data-dest-id]") ?? null;
  }

  #highlighted: HTMLElement | null = null;
  #highlight(el: HTMLElement | null) {
    if (el === this.#highlighted) return;
    this.#highlighted?.classList.remove("ring-2", "ring-primary");
    el?.classList.add("ring-2", "ring-primary");
    this.#highlighted = el;
  }

  #onPointerDown = (e: PointerEvent) => {
    const port = (e.target as HTMLElement).closest<HTMLElement>("[data-port-source]");
    if (!port) return;
    e.preventDefault();
    const sourceId = port.dataset.portSource!;
    const from = this.layout.ports[sourceId] ?? this.#point(e);
    this.drag = { sourceId, from, to: this.#point(e) };
    const move = (ev: PointerEvent) => {
      if (this.drag) this.drag = { ...this.drag, to: this.#point(ev) };
      this.#highlight(this.#dropTarget(ev));
    };
    const up = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      const target = this.#dropTarget(ev);
      this.#highlight(null);
      this.drag = null;
      if (target) void this.#connect(sourceId, target.dataset.destId!);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  async #connect(sourceId: string, destinationId: string) {
    const { snapshot, space } = this.store;
    if (!snapshot || !space) return;
    if (snapshot.flows.some((f) => f.source_id === sourceId && f.destination_id === destinationId)) {
      this.store.toast("info", "These are already connected");
      return;
    }
    await this.store.save("flow", newFlow(space.id, sourceId, destinationId));
  }

  // ---- line menu ----

  #closeMenu = (e: PointerEvent) => {
    if (this.menu && !(e.target as HTMLElement).closest("[data-flow-menu]")) this.menu = null;
  };

  #openMenu(e: MouseEvent, flow: Flow) {
    const origin = this.#board.getBoundingClientRect();
    this.menu = { flow, at: { x: e.clientX - origin.left, y: e.clientY - origin.top } };
  }

  #menuAction(action: () => void) {
    this.menu = null;
    action();
  }

  #renderMenu() {
    if (!this.menu) return nothing;
    const { flow, at } = this.menu;
    const fs = flowStatus(this.store.status, flow.id);
    return html`
      <ul data-flow-menu class="menu menu-sm absolute z-20 w-52 rounded-box bg-base-100 shadow-lg border border-base-300 pointer-events-auto" style="left:${at.x}px;top:${at.y}px">
        <li><button ?disabled=${!fs || fs.to_transfer + fs.failed === 0} @click=${() => this.#menuAction(() => void this.store.runFlow(flow.id))}><omb-icon name="play"></omb-icon>Run this flow</button></li>
        <li><button @click=${() => this.#menuAction(() => this.store.open({ type: "preview", flowId: flow.id }))}><omb-icon name="images"></omb-icon>Preview files</button></li>
        <li><button class="text-error" @click=${() => this.#menuAction(() => void this.store.remove("flow", flow.id))}><omb-icon name="unplug"></omb-icon>Disconnect</button></li>
      </ul>
    `;
  }

  // ---- render ----

  #badge(line: Line) {
    const fs = flowStatus(this.store.status, line.flow.id);
    if (!fs) return nothing;
    let content;
    let tone;
    let t = 0.5;
    if (line.state === "pending" && fs.to_transfer) {
      content = html`<omb-icon name="clock" class="size-3.5"></omb-icon>${formatCount(fs.to_transfer)}`;
      tone = "badge-warning";
    } else if (line.state === "error") {
      content = html`<omb-icon name="triangle-alert" class="size-3.5"></omb-icon>${failureLabel(fs.failed, fs.error)}`;
      tone = "badge-error";
      t = 0.3;
    } else return nothing;
    const p = pointOnCurve(line.from, line.to, t);
    return html`<button
      class="badge ${tone} badge-soft border-current gap-1 absolute -translate-x-1/2 -translate-y-1/2 pointer-events-auto bg-base-200 whitespace-nowrap"
      style="left:${p.x}px;top:${p.y}px"
      @click=${() => this.store.open({ type: "preview", flowId: line.flow.id, category: line.state === "error" ? "error" : "to_transfer" })}
    >${content}</button>`;
  }

  override render() {
    const lines = this.#lines();
    const selected = this.store.selectedSourceId;
    const dim = (l: Line) => (selected && l.flow.source_id !== selected ? 0.2 : 1);
    return html`
      <svg class="absolute inset-0 w-full h-full overflow-visible">
        ${lines.map(
          (l) => svg`
            <g opacity=${dim(l)}>
              <path d=${curve(l.from, l.to)} fill="none" stroke=${FLOW_COLOR[l.state]} stroke-width="2.5" stroke-linecap="round"></path>
              <path d=${curve(l.from, l.to)} fill="none" stroke="transparent" stroke-width="14" class="pointer-events-auto cursor-pointer"
                @click=${(e: MouseEvent) => this.#openMenu(e, l.flow)}><title>Click for options</title></path>
              <circle cx=${l.to.x} cy=${l.to.y} r="4.5" fill=${FLOW_COLOR[l.state]}></circle>
            </g>`,
        )}
        ${this.drag
          ? svg`<path d=${curve(this.drag.from, this.drag.to)} fill="none" stroke="var(--color-primary)" stroke-width="2.5" stroke-dasharray="6 5"></path>`
          : nothing}
      </svg>
      ${lines.map((l) => (dim(l) === 1 ? this.#badge(l) : nothing))}
      ${this.#renderMenu()}
    `;
  }
}
