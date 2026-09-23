import React from "react";
import ReactDOM from "react-dom/client";
import { DesktopApp } from "./desktop-app";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <DesktopApp />
  </React.StrictMode>,
);
