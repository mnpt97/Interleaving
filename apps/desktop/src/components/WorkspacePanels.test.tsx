import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { WorkspacePanels } from "./WorkspacePanels";
import { useScenarioStore } from "../features/scenarios/store";
import { useRunStore } from "../features/runs/store";

vi.mock("../lib/tauri", async (original) => ({ ...await original<typeof import("../lib/tauri")>(), desktopAvailable: true }));
vi.mock("./ThemeControl", () => ({ ThemeControl: () => null }));
vi.mock("../features/scenarios/ScenarioCanvas", () => ({ ScenarioCanvas: () => <div>Workflow canvas</div> }));
const initialScenario = useScenarioStore.getState();
const initialRun = useRunStore.getState();
beforeEach(() => {
  useScenarioStore.setState({ ...initialScenario, status: "valid", loadExample: vi.fn().mockResolvedValue(undefined), validate: vi.fn().mockResolvedValue(undefined) }, true);
  useRunStore.setState(initialRun, true);
});
afterEach(() => {
  useScenarioStore.setState(initialScenario, true);
  useRunStore.setState(initialRun, true);
});

describe("workspace panel tabs", () => {
  it("places Validate inside Validation and switches to Run's actions", async () => {
    const user = userEvent.setup(); render(<App />);
    expect(within(screen.getByRole("region", { name: "Scenario actions" })).queryByRole("button", { name: "Validate" })).not.toBeInTheDocument();
    const validation = screen.getByRole("tabpanel", { name: "Validation" });
    await user.click(within(validation).getByRole("button", { name: "Validate" }));
    expect(useScenarioStore.getState().validate).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("tab", { name: "Run" }));
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Validate" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("tabpanel", { name: "Run" })).getByRole("button", { name: "Run" })).toBeEnabled();
  });

  it("supports arrow-key navigation and retains run events and selection across tabs", async () => {
    const user = userEvent.setup();
    useRunStore.setState({ status: "running", runId: "active", selected: 1, events: [{ runId: "active", sequence: 1, elapsedNs: 1000, userId: "user", nodeId: "health", type: "response.received", payload: { status: 200, headers: {}, body: { value: { status: "ok" }, bytes: 15, truncated: false, omitted: false } } }] });
    render(<WorkspacePanels />);
    screen.getByRole("tab", { name: "Validation" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Run" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(screen.getByRole("heading", { name: "response.received · HTTP 200" })).toBeInTheDocument();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Validation" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("heading", { name: "response.received · HTTP 200" })).toBeInTheDocument();
    expect(useRunStore.getState().status).toBe("running");
    expect(useRunStore.getState().events).toHaveLength(1);
  });

  it("keeps Validate disabled during pending work", () => {
    useScenarioStore.setState({ busy: true }); render(<WorkspacePanels />);
    expect(screen.getByRole("button", { name: "Validate" })).toBeDisabled();
  });

  it("shows a warning and opens Validation on failed loads, including consecutive invalid files", async () => {
    const user = userEvent.setup(); render(<WorkspacePanels />);
    await user.click(screen.getByRole("tab", { name: "Run" }));
    act(() => useScenarioStore.setState({ status: "invalid", fileRevision: 1 }));
    const validation = screen.getByRole("tab", { name: "Validation" });
    expect(validation).toHaveAttribute("aria-selected", "true");
    expect(validation).toHaveAccessibleDescription("Validation failed");
    expect(validation.querySelector(".validation-tab-warning")).toBeInTheDocument();
    expect(validation.querySelector(".validation-tab-check")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Run" }));
    expect(screen.getByRole("tab", { name: "Run" })).toHaveAttribute("aria-selected", "true");
    act(() => useScenarioStore.setState({ busy: true }));
    expect(screen.getByRole("tab", { name: "Run" })).toHaveAttribute("aria-selected", "true");
    act(() => useScenarioStore.setState({ busy: false, status: "invalid", fileRevision: 2 }));
    expect(validation).toHaveAttribute("aria-selected", "true");
  });

  it("shows a check for valid loads without changing the selected tab, including after a failure", async () => {
    const user = userEvent.setup(); render(<WorkspacePanels />);
    await user.click(screen.getByRole("tab", { name: "Run" }));
    act(() => useScenarioStore.setState({ status: "valid", fileRevision: 1 }));
    const validation = screen.getByRole("tab", { name: "Validation" });
    expect(validation).toHaveAccessibleDescription("Validation passed");
    expect(validation.querySelector(".validation-tab-check")).toHaveAttribute("stroke", "currentColor");
    expect(screen.getByRole("tab", { name: "Run" })).toHaveAttribute("aria-selected", "true");
    act(() => useScenarioStore.setState({ status: "invalid", fileRevision: 2 }));
    await user.click(screen.getByRole("tab", { name: "Run" }));
    act(() => useScenarioStore.setState({ status: "valid", fileRevision: 3 }));
    expect(validation.querySelector(".validation-tab-warning")).not.toBeInTheDocument();
    expect(validation.querySelector(".validation-tab-check")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Run" })).toHaveAttribute("aria-selected", "true");
  });

  it("shows no result icon before validation", () => {
    useScenarioStore.setState({ status: "unchecked" }); render(<WorkspacePanels />);
    const validation = screen.getByRole("tab", { name: "Validation" });
    expect(validation).toHaveAccessibleDescription("Not validated");
    expect(validation.querySelector("svg")).not.toBeInTheDocument();
  });

  it("shows a rotating loader during validation, then restores the result icon", () => {
    useScenarioStore.setState({ busy: true, validating: true }); render(<WorkspacePanels />);
    const validation = screen.getByRole("tab", { name: "Validation" });
    expect(validation).toHaveAccessibleDescription("Validating scenario");
    expect(validation.querySelector(".validation-tab-loading")).toHaveClass("spin");
    expect(validation.querySelector(".validation-tab-check")).not.toBeInTheDocument();
    act(() => useScenarioStore.setState({ busy: false, validating: false, status: "valid" }));
    expect(validation.querySelector(".validation-tab-loading")).not.toBeInTheDocument();
    expect(validation.querySelector(".validation-tab-check")).toBeInTheDocument();
  });

  it("does not show a validation loader for unrelated busy work", () => {
    useScenarioStore.setState({ busy: true, validating: false }); render(<WorkspacePanels />);
    const validation = screen.getByRole("tab", { name: "Validation" });
    expect(validation.querySelector(".validation-tab-loading")).not.toBeInTheDocument();
    expect(validation.querySelector(".validation-tab-check")).toBeInTheDocument();
  });
});
