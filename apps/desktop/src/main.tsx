import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./style.css";

const root = createRoot(document.getElementById("root")!);
if (import.meta.env.DEV && new URLSearchParams(location.search).has("variant")) {
  import("./DesignPrototype").then(({ DesignPrototype }) => root.render(<DesignPrototype />));
} else root.render(<App />);
