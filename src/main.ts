import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./app.css";
import App from "./App.svelte";
import Splash from "./Splash.svelte";
import SignInWindow from "./SignInWindow.svelte";

/** La etiqueta de la ventana decide qué interfaz monta el mismo bundle. */
function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    // Si se abre el frontend en un navegador normal, no hay ventana Tauri.
    return "";
  }
}

const target = document.getElementById("app")!;
const label = windowLabel();

if (label === "splash") {
  mount(Splash, { target });
} else if (label === "signin") {
  mount(SignInWindow, { target });
} else {
  mount(App, { target });
}
