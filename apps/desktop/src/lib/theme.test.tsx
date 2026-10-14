import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { isTauri } from "@tauri-apps/api/core";
import { ThemeControl } from "../components/ThemeControl";
import { initializeTheme, ThemeProvider } from "./theme";

const native = vi.hoisted(() => ({ setTheme: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: vi.fn(() => false) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));

function system(dark: boolean) {
  const media = Object.assign(new EventTarget(), { matches: dark });
  vi.stubGlobal("matchMedia", vi.fn(() => media));
  return (next: boolean) => act(() => { media.matches = next; media.dispatchEvent(new Event("change")); });
}
function picker() { return render(<ThemeProvider><ThemeControl /></ThemeProvider>); }
function choose(value: string) { fireEvent.change(screen.getByRole("combobox", { name: "Color theme" }), { target: { value } }); }

beforeEach(() => {
  localStorage.clear(); document.documentElement.className = ""; delete document.documentElement.dataset.theme;
  vi.mocked(isTauri).mockReturnValue(false); native.setTheme.mockClear(); system(false);
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe("color theme", () => {
  it("follows OS appearance by default and stops listening after unmount", () => {
    const change = system(true); const view = picker();
    expect(screen.getByRole("combobox", { name: "Color theme" })).toHaveValue("system");
    expect(document.documentElement).toHaveClass("dark");
    expect(document.documentElement.style.colorScheme).toBe("dark");
    change(false); expect(document.documentElement).not.toHaveClass("dark");
    view.unmount(); change(true); expect(document.documentElement).not.toHaveClass("dark");
  });

  it("lets a manual choice override the OS and restores system following", () => {
    const change = system(false); picker(); choose("dark");
    expect(localStorage.getItem("interleaving.theme")).toBe("dark");
    change(true); change(false); expect(document.documentElement).toHaveClass("dark");
    choose("light"); change(true); expect(document.documentElement).not.toHaveClass("dark");
    choose("system"); expect(document.documentElement).toHaveClass("dark");
    expect(localStorage.getItem("interleaving.theme")).toBe("system");
  });

  it("applies saved appearance before mounting and restores it after reload", () => {
    localStorage.setItem("interleaving.theme", "dark"); initializeTheme();
    expect(document.documentElement).toHaveClass("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    picker(); expect(screen.getByRole("combobox", { name: "Color theme" })).toHaveValue("dark");
  });

  it("falls back to the system for unknown saved values", () => {
    system(true); localStorage.setItem("interleaving.theme", "unknown"); initializeTheme(); picker();
    expect(document.documentElement).toHaveClass("dark");
    expect(screen.getByRole("combobox", { name: "Color theme" })).toHaveValue("system");
  });

  it("remains usable when storage or matchMedia is unavailable", () => {
    vi.stubGlobal("matchMedia", undefined);
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("Unavailable"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("Unavailable"); });
    initializeTheme(); picker(); choose("dark");
    expect(document.documentElement).toHaveClass("dark");
    expect(screen.getByRole("combobox", { name: "Color theme" })).toHaveValue("dark");
  });

  it("synchronizes native window appearance with manual and system choices", () => {
    vi.mocked(isTauri).mockReturnValue(true); picker();
    expect(native.setTheme).toHaveBeenLastCalledWith(null);
    choose("dark"); expect(native.setTheme).toHaveBeenLastCalledWith("dark");
    choose("light"); expect(native.setTheme).toHaveBeenLastCalledWith("light");
    choose("system"); expect(native.setTheme).toHaveBeenLastCalledWith(null);
  });
});
