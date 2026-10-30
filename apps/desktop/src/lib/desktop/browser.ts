import { unavailable, type DesktopHost } from './host';
/** Explicit preview host; never silently chosen after a native bridge failure. */
export const browserHost: DesktopHost = {
  kind: 'browser',
  native: false,
  available: false,
  startScenario: async () => unavailable(),
  cancelScenario: async () => unavailable(),
  saveRunReport: async () => unavailable(),
  validateSource: async () => unavailable(),
  openScenarioFile: async () => unavailable(),
  readScenarioFile: async () => unavailable(),
  saveScenarioJson: async () => unavailable(),
  setTheme: async () => {},
  onCloseRequested: async () => () => {},
};
