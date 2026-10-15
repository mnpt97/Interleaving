import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { initializeTheme, ThemeProvider } from './lib/theme';

initializeTheme();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </React.StrictMode>
);
