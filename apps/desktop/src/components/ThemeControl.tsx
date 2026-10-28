import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { SunMoon } from 'lucide-react';
import { useTheme, type ThemePreference } from '../lib/theme';

export function ThemeControl() {
  const { preference, setPreference } = useTheme();
  return (
    <div className="theme-control">
      <SunMoon size={16} aria-hidden="true" />
      <Select value={preference} onValueChange={(value) => setPreference(value as ThemePreference)}>
        <SelectTrigger aria-label="Color theme">
          <SelectValue />
        </SelectTrigger>
        <SelectContent align="end">
          <SelectItem value="system">System</SelectItem>
          <SelectItem value="light">Light</SelectItem>
          <SelectItem value="dark">Dark</SelectItem>
        </SelectContent>
      </Select>
    </div>
  );
}
