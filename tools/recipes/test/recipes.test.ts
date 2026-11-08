import { test } from "node:test";
import assert from "node:assert/strict";
import { advance, button, capture, click, expect, fill, flow, matrix, recipe, suite, validate } from "../src/index.ts";
test("versioned matrices keep fixtures independent", () => {
  const plan = suite(matrix([{ name: "desktop", width: 1440 }, { name: "compact", width: 820 }], viewport => recipe(viewport.name, {
    state: { query: "" }, viewport, steps: [...flow("search", [fill("Search", "hello")]), capture("result")],
  })));
  validate(plan);
  assert.deepEqual(plan.cases.map(c => c.viewport.width), [1440, 820]);
  assert.deepEqual(plan.cases[0].steps[0].flow, ["search"]);
  assert.ok(plan.cases[0].steps[0].source?.line);
  const cloned = structuredClone(plan);
  cloned.cases[0].state.query = "mutated";
  assert.equal(cloned.cases[1].state.query, "");
  assert.equal(plan.cases[0].state.query, "");
});
test("flow preserves nested names and authored source locations without aliasing", () => {
  const action = fill("Search", "hello");
  const steps = flow("outer", flow("inner", [action]));
  assert.deepEqual(steps[0].flow, ["outer", "inner"]);
  assert.equal(steps[0].source?.file, action.source?.file);
  steps[0].flow.push("changed");
  assert.deepEqual(action.flow, []);
});
test("invalid plans are rejected before output", () => {
  const make = (steps = [capture("view")]) => suite([recipe("case", { state: {}, steps })]);
  for (const plan of [
    suite([]),
    suite([recipe("../bad", {state: {}, steps: []})]),
    make([capture("duplicate"), capture("duplicate")]),
    make([capture("_failure")]),
    make([capture("../escape")]),
    make([advance(-1)]),
    make([expect(button("x")).count(-1)]),
    suite([recipe("case", {state: {}, viewport: {scale: 0}, steps: []})]),
    suite([recipe("case", {state: {number: NaN}, steps: []})]),
    suite([recipe("case", {state: {date: new Date()}, steps: []})]),
  ]) assert.throws(() => validate(plan));
  const one = make();
  assert.throws(() => validate(suite([one.cases[0], one.cases[0]])));
  assert.throws(() => click({role: "button", label_prefix: "ambiguous"}));
});
test("matrix copies its input before each builder", () => {
  const entries = [{name:"a", state: {count: 1}}];
  const result = matrix(entries, item => {
    item.state.count++;
    return recipe(item.name, {state: item.state, steps: []});
  });
  assert.equal(entries[0].state.count, 1);
  assert.equal(result[0].state.count, 2);
});
