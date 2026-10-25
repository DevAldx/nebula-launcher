import { app } from "./app.svelte";

/**
 * Acción de Svelte: pide el fondo de una instancia solo cuando la tarjeta
 * entra en pantalla (con 300 px de margen), en lugar de cargar de golpe todas
 * las imágenes de la biblioteca.
 *
 * Cada fondo llega por IPC como `data:` URL y se queda en memoria, así que
 * pedirlos todos a la vez en bibliotecas grandes cuesta RAM y tiempo de
 * apertura sin que el usuario llegue a verlos.
 */
export function lazyBackground(node: HTMLElement, instanceId: string) {
  let id = instanceId;
  let observer: IntersectionObserver | null = null;

  const load = () => {
    if (id) void app.ensureBackground(id);
  };

  if (typeof IntersectionObserver === "undefined") {
    load(); // entorno sin IO (tests): cargar de inmediato
  } else {
    observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          load();
          observer?.disconnect();
          observer = null;
        }
      },
      { rootMargin: "300px" },
    );
    observer.observe(node);
  }

  return {
    update(next: string) {
      if (next !== id) {
        id = next;
        if (!observer) load();
      }
    },
    destroy() {
      observer?.disconnect();
      observer = null;
    },
  };
}
