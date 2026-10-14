import { beforeEach, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NodeInspector } from "./NodeInspector";
import { ValidationPanel } from "./ValidationPanel";
import { exampleDocument, useScenarioStore } from "./store";

beforeEach(() => useScenarioStore.setState({ document: structuredClone(exampleDocument), selectedId: "health", diagnostics: [], status: "valid", layoutDirty: false }));

it("edits positions without changing workflow semantics", async () => {
  const workflow = useScenarioStore.getState().document!.workflow;
  render(<NodeInspector />);
  fireEvent.change(screen.getByLabelText("X"), { target: { value: "144" } });
  fireEvent.click(screen.getByRole("button", { name: "Apply position" }));
  await waitFor(() => expect(useScenarioStore.getState().document?.layout?.positions.health.x).toBe(144));
  expect(useScenarioStore.getState().document!.workflow).toBe(workflow);
});

it("shows a recovery message for an invalid coordinate", async () => {
  render(<NodeInspector />); fireEvent.change(screen.getByLabelText("X"), { target: { value: "" } }); fireEvent.click(screen.getByRole("button", { name: "Apply position" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("finite numeric coordinates");
});

it("selects the relevant node from a diagnostic", () => {
  useScenarioStore.setState({ selectedId: null, status: "invalid", diagnostics: [{ category: "validation", code: "invalidTimeout", message: "Timeout must be positive.", fieldPath: "/workflow/nodes/1/config/timeoutMs", nodeId: "health", filePath: null, line: null, column: null }] });
  render(<ValidationPanel />); fireEvent.click(screen.getByRole("button", { name: /Timeout must be positive/ })); expect(useScenarioStore.getState().selectedId).toBe("health");
});
