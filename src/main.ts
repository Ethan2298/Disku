import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App.svelte";
import "./app.css";

if (navigator.userAgent.includes("Windows")) {
  document.documentElement.classList.add("platform-windows");
}

const appWindow = getCurrentWindow();
appWindow.onResized(async () => {
  const fullscreen = await appWindow.isFullscreen();
  document.documentElement.classList.toggle("fullscreen", fullscreen);
});

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
