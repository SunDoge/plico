import { fileURLToPath } from "node:url";

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type Role = "button" | "textbox" | "switch" | "checkbox" | "slider" | "combobox" | "option" | "tab" | "link" | "list" | "listitem";
export interface Viewport { width: number; height: number; scale: number }
export interface Location { file: string; line: number; column: number }
export interface Selector { role?: Role; label?: string; label_prefix?: string; occurrence?: number }
export type Command =
  | { action: "click" | "focus"; role: Role; label: string; occurrence?: number }
  | { action: "text"; text: string }
  | { action: "key"; key: string }
  | { action: "set_value"; label: string; value: number }
  | { action: "pointer_move"; x: number; y: number }
  | { action: "pointer_leave" | "snapshot" };
export type NodeExpectation =
  | { kind: "exists" | "absent" }
  | { kind: "count"; count: number }
  | { kind: "enabled"; enabled: boolean }
  | { kind: "value"; value: string }
  | { kind: "selected"; selected: boolean };
export type Assertion =
  | { kind: "nodes"; selector: Selector; expect: NodeExpectation }
  | { kind: "text"; text: string; present: boolean };
export type Operation =
  | { op: "command"; command: Command }
  | { op: "fill"; label: string; text: string; occurrence?: number }
  | { op: "advance"; milliseconds: number }
  | { op: "assert"; assertion: Assertion }
  | { op: "wait"; assertion: Assertion; timeout_ms: number; interval_ms?: number }
  | { op: "capture"; name: string }
  | { op: "event"; event: Json };
export interface Step { source?: Location; flow: string[]; operation: Operation }
export interface Recipe<S> { name: string; state: S; viewport: Viewport; steps: Step[] }
export interface Plan<S> { version: 1; cases: Recipe<S>[] }
const ownFile = fileURLToPath(import.meta.url);
function location(): Location | undefined {
  for (const line of new Error().stack?.split("\n").slice(1) ?? []) {
    const match = line.match(/(?:\(|at )((?:file:\/\/|\/|[A-Za-z]:[\\/]).*):(\d+):(\d+)\)?$/);
    if (!match) continue;
    const file = match[1].startsWith("file:") ? fileURLToPath(match[1]) : match[1];
    if (file === ownFile) continue;
    return { file, line: Number(match[2]), column: Number(match[3]) };
  }
}
function step(operation: Operation): Step { return { source: location(), flow: [], operation }; }
export const button = (label: string): Selector => ({ role: "button", label });
export const textbox = (label: string): Selector => ({ role: "textbox", label });
export const buttonsStartingWith = (label_prefix: string): Selector => ({ role: "button", label_prefix });
export function click(selector: Selector): Step {
  if (!selector.role || selector.label === undefined || selector.label_prefix !== undefined) throw new Error("click requires an exact role and label");
  return step({ op: "command", command: { action: "click", role: selector.role, label: selector.label, occurrence: selector.occurrence } });
}
export function focus(selector: Selector): Step {
  if (!selector.role || selector.label === undefined || selector.label_prefix !== undefined) throw new Error("focus requires an exact role and label");
  return step({ op: "command", command: { action: "focus", role: selector.role, label: selector.label, occurrence: selector.occurrence } });
}
export const fill = (label: string, text: string, occurrence?: number): Step => step({ op: "fill", label, text, occurrence });
export const typeText = (text: string): Step => step({ op: "command", command: { action: "text", text } });
export const key = (key: string): Step => step({ op: "command", command: { action: "key", key } });
export const setValue = (label: string, value: number): Step => step({ op: "command", command: { action: "set_value", label, value } });
export const advance = (milliseconds: number): Step => step({ op: "advance", milliseconds });
export const capture = (name: string): Step => step({ op: "capture", name });
export const event = <E extends Json>(value: E): Step => step({ op: "event", event: value });
export function expect(selector: Selector) {
  const nodes = (value: NodeExpectation) => step({ op: "assert", assertion: { kind: "nodes", selector: structuredClone(selector), expect: value } });
  return {
    exists: () => nodes({ kind: "exists" }),
    absent: () => nodes({ kind: "absent" }),
    count: (count: number) => nodes({ kind: "count", count }),
    enabled: (enabled = true) => nodes({ kind: "enabled", enabled }),
    value: (value: string) => nodes({ kind: "value", value }),
    selected: (selected = true) => nodes({ kind: "selected", selected }),
  };
}
export const expectText = (text: string, present = true): Step => step({ op: "assert", assertion: { kind: "text", text, present } });
/** Poll an assertion by advancing deterministic virtual time, never wall-clock time. */
export function waitFor(assertionStep: Step, options: { timeoutMs?: number; intervalMs?: number } = {}): Step {
  if (assertionStep.operation.op !== "assert") throw new Error("waitFor requires an expect(...) or expectText(...) assertion");
  return step({
    op: "wait",
    assertion: structuredClone(assertionStep.operation.assertion),
    timeout_ms: options.timeoutMs ?? 5_000,
    interval_ms: options.intervalMs ?? 16,
  });
}
/** Expands a reusable flow while preserving its name and each authored source location. */
export function flow(name: string, steps: readonly Step[]): Step[] {
  return steps.map(s => ({ ...structuredClone(s), flow: [name, ...s.flow] }));
}
export function recipe<S>(name: string, options: { state: S; viewport?: Partial<Viewport>; steps: readonly Step[] }): Recipe<S> {
  return structuredClone({ name, state: options.state,
    viewport: { width: options.viewport?.width ?? 1440, height: options.viewport?.height ?? 1000, scale: options.viewport?.scale ?? 1 }, steps: [...options.steps] });
}
/** Generation-time expansion; no runtime branches or shared mutable case state. */
export function matrix<T, S>(values: readonly T[], build: (value: T) => Recipe<S>): Recipe<S>[] {
  return values.map(value => build(structuredClone(value)));
}
export const suite = <S>(cases: readonly Recipe<S>[]): Plan<S> => structuredClone({ version: 1, cases: [...cases] });
/** Reject values JSON.stringify would silently discard or change. Optional object fields may be undefined. */
function jsonValue(value: unknown, path: string, seen: Set<object>): void {
  if (value === null || typeof value === "string" || typeof value === "boolean") return;
  if (typeof value === "number" && Number.isFinite(value)) return;
  if (typeof value !== "object" || value === null) throw new Error(`${path}: not a JSON value`);
  if (seen.has(value)) throw new Error(`${path}: cyclic JSON value`);
  if (!Array.isArray(value) && Object.getPrototypeOf(value) !== Object.prototype) throw new Error(`${path}: expected a plain JSON object`);
  seen.add(value);
  for (const [key, child] of Object.entries(value)) {
    if (child === undefined && !Array.isArray(value)) continue;
    jsonValue(child, `${path}.${key}`, seen);
  }
  seen.delete(value);
}
const safeName = (name: string) => /^[a-zA-Z0-9_-]+$/.test(name);
export function validate(plan: Plan<unknown>): void {
  jsonValue(plan, "plan", new Set());
  if (plan.version !== 1 || !Array.isArray(plan.cases) || !plan.cases.length) throw new Error("expected version 1 with nonempty cases");
  const names = new Set<string>();
  for (const item of plan.cases) {
    if (!safeName(item.name) || names.has(item.name)) throw new Error(`unsafe or duplicate case name: ${item.name}`);
    names.add(item.name);
    if (item.state === undefined) throw new Error(`missing fixture state: ${item.name}`);
    const v = item.viewport;
    if (![v.width, v.height].every(n => Number.isSafeInteger(n) && n > 0) || !(v.scale > 0 && v.scale <= 8) || v.width * v.scale > 8192 || v.height * v.scale > 8192) throw new Error(`invalid viewport: ${item.name}`);
    const captures = new Set<string>();
    for (const { operation } of item.steps) {
      if (operation.op === "capture") {
        if (!safeName(operation.name) || operation.name === "_failure" || captures.has(operation.name)) throw new Error(`unsafe, reserved or duplicate capture: ${operation.name}`);
        captures.add(operation.name);
      } else if (operation.op === "advance" && (!Number.isSafeInteger(operation.milliseconds) || operation.milliseconds < 0)) throw new Error("advance requires a nonnegative safe integer");
      else if (operation.op === "wait") {
        if (!Number.isSafeInteger(operation.timeout_ms) || operation.timeout_ms < 1 || operation.timeout_ms > 300_000)
          throw new Error("wait requires timeout_ms in 1..=300000");
        const interval = operation.interval_ms ?? 16;
        if (!Number.isSafeInteger(interval) || interval < 1 || interval > operation.timeout_ms)
          throw new Error("wait requires interval_ms in 1..=timeout_ms");
      }
      if ((operation.op === "assert" || operation.op === "wait") && operation.assertion.kind === "nodes") {
        const { selector, expect } = operation.assertion;
        if (selector.label !== undefined && selector.label_prefix !== undefined) throw new Error("ambiguous selector label/prefix");
        if (selector.occurrence !== undefined && (!Number.isSafeInteger(selector.occurrence) || selector.occurrence < 0)) throw new Error("invalid occurrence");
        if (expect.kind === "count" && (!Number.isSafeInteger(expect.count) || expect.count < 0)) throw new Error("invalid count");
      }
    }
  }
}
