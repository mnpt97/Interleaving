import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { RunPanel } from "./RunPanel";
import { useRunStore } from "./store";
const initial = useRunStore.getState();
afterEach(() => useRunStore.setState(initial, true));
describe("HTTP response inspector", () => {
  it("shows HTTP 404 as a received response and displays redacted details", () => {
    useRunStore.setState({ status: "passed", events: [{runId:"run",sequence:1,elapsedNs:1000000,userId:"user",nodeId:"health",type:"response.received",payload:{status:404,headers:{"set-cookie":"[REDACTED]"},body:{value:{error:"not found"},bytes:10,truncated:false,omitted:false}}}] });
    render(<RunPanel />); fireEvent.click(screen.getByRole("button",{name:/response.received/}));
    expect(screen.getByRole("heading",{name:"response.received · HTTP 404"})).toBeInTheDocument(); expect(screen.getByLabelText("Request and response inspector")).toHaveTextContent("[REDACTED]"); expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("shows transport failures clearly", () => {
    useRunStore.setState({status:"error",error:"timeout: Request timed out"}); render(<RunPanel />); expect(screen.getByRole("alert")).toHaveTextContent("timeout: Request timed out");
  });
});
