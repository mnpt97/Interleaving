import { describe, expect, it } from "vitest";
import { moveNode, toFlowEdges, toFlowNodes } from "./adapters";
import { exampleDocument } from "./store";

describe("React Flow adapters", () => {
  it("keeps layout updates out of executable workflow and leaves input immutable", () => {
    const original = structuredClone(exampleDocument);
    const moved = moveNode(original, "health", { x: 120, y: 240 });
    expect(moved.workflow).toBe(original.workflow);
    expect(original).toEqual(exampleDocument);
    expect(toFlowNodes(moved, "health").find((n) => n.id === "health")).toMatchObject({ selected: true, position: { x: 120, y: 240 } });
    expect(toFlowEdges(moved)).toEqual(toFlowEdges(original));
    expect(moved.workflow.edges[0]).not.toHaveProperty("id");
  });
  it("supplies deterministic positions when layout is absent", () => {
    const document = { ...exampleDocument, layout: undefined };
    expect(toFlowNodes(document, null).map((node) => node.position)).toEqual([{ x: 80, y: 160 }, { x: 340, y: 160 }, { x: 600, y: 160 }]);
  });
  it("rejects nonfinite positions and unknown node IDs", () => {
    expect(moveNode(exampleDocument, "missing", { x: 1, y: 1 })).toBe(exampleDocument);
    expect(moveNode(exampleDocument, "health", { x: NaN, y: 1 })).toBe(exampleDocument);
  });
});
