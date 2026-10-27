import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./app.css";
import App from "./App.svelte";
import SignInWindow from "./SignInWindow.svelte";

/** La ventana de inicio de sesión carga el mismo bundle, pero monta otra UI. */
function isSignInWindow(): boolean {
  try {
    return getCurrentWindow().label === "signin";
  } catch {
    // Si se abre el frontend en un navegador normal, no hay ventana Tauri.
    return false;
  }
}

const target = document.getElementById("app")!;
if (isSignInWindow()) {
  mount(SignInWindow, { target });
} else {
  mount(App, { target });
}
