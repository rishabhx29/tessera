import React from 'react';
import ReactDOM from 'react-dom/client';
import '@fontsource/instrument-serif/400.css';
import '@fontsource/instrument-serif/400-italic.css';
import '@fontsource-variable/inter-tight';
import '@fontsource-variable/jetbrains-mono';
import App from './App';
import './index.css';

const container = document.getElementById('root')!;
const tree = (
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// The build prerenders the page, so the usual case is hydration.
if (container.hasChildNodes()) {
  ReactDOM.hydrateRoot(container, tree);
} else {
  ReactDOM.createRoot(container).render(tree);
}
