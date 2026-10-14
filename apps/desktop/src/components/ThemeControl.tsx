import { SunMoon } from "lucide-react";
import { useTheme, type ThemePreference } from "../lib/theme";

export function ThemeControl() {
  const { preference, setPreference } = useTheme();
  return <label className="theme-control"><SunMoon size={16} aria-hidden="true" /><span className="sr-only">Color theme</span><select value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}><option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option></select></label>;
}
