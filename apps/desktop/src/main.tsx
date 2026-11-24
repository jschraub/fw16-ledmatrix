import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./style.css";

const root = createRoot(document.getElementById("root")!);
if (import.meta.env.DEV && new URLSearchParams(location.search).has("variant")) {
  import("./prototype/DesignStudies").then(({DesignStudies}) => root.render(<DesignStudies />));
} else root.render(<App />);
