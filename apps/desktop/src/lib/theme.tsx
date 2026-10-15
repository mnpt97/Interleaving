import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export type ThemePreference = 'system' | 'light' | 'dark';
type ResolvedTheme = 'light' | 'dark';
const storageKey = 'interleaving.theme';
const mediaQuery = '(prefers-color-scheme: dark)';

function readPreference(): ThemePreference {
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved === 'light' || saved === 'dark') return saved;
  } catch {
    /* The theme still works when storage is unavailable. */
  }
  return 'system';
}

function systemTheme(): ResolvedTheme {
  return window.matchMedia?.(mediaQuery).matches ? 'dark' : 'light';
}
function applyTheme(theme: ResolvedTheme) {
  document.documentElement.classList.toggle('dark', theme === 'dark');
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

// Apply before React mounts so startup uses the saved or system appearance.
export function initializeTheme() {
  const preference = readPreference();
  applyTheme(preference === 'system' ? systemTheme() : preference);
}

interface ThemeContextValue {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (theme: ThemePreference) => void;
}
const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState(readPreference);
  const [system, setSystem] = useState(systemTheme);
  const resolvedTheme = preference === 'system' ? system : preference;
  const setPreference = useCallback((theme: ThemePreference) => {
    setPreferenceState(theme);
    try {
      localStorage.setItem(storageKey, theme);
    } catch {
      /* Keep the in-memory choice. */
    }
  }, []);

  useLayoutEffect(() => {
    applyTheme(resolvedTheme);
  }, [resolvedTheme]);
  useEffect(() => {
    const media = window.matchMedia?.(mediaQuery);
    const update = () => setSystem(media?.matches ? 'dark' : 'light');
    update();
    media?.addEventListener('change', update);
    return () => media?.removeEventListener('change', update);
  }, []);
  useEffect(() => {
    if (isTauri())
      void getCurrentWindow()
        .setTheme(preference === 'system' ? null : resolvedTheme)
        .catch((error: unknown) => console.warn('Could not update native window appearance.', error));
  }, [preference, resolvedTheme]);

  const value = useMemo(
    () => ({ preference, resolvedTheme, setPreference }),
    [preference, resolvedTheme, setPreference]
  );
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const theme = useContext(ThemeContext);
  if (!theme) throw new Error('useTheme requires ThemeProvider');
  return theme;
}
