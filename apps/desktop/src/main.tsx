import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import "@neuro/ui/styles.css";
import { LoomUiProvider } from "./components/LoomUiProvider";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Loom desktop root element was not found");
}

createRoot(rootElement).render(
  <React.StrictMode>
    <LoomUiProvider><App /></LoomUiProvider>
  </React.StrictMode>,
);
