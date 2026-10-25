import { api, type NebulaUpdateStatus } from "./api";

/**
 * Estado compartido del actualizador. Lo usan el diálogo (que se abre solo al
 * detectar una versión nueva) y el botón manual de Configuración, para que
 * ambos hablen del mismo `status` y no existan dos comprobaciones a la vez.
 */
class UpdatesState {
  open = $state(false);
  checking = $state(false);
  installing = $state(false);
  error = $state("");
  status = $state<NebulaUpdateStatus | null>(null);
  /** Bytes descargados de la actualización en curso y tamaño total si se conoce. */
  downloaded = $state(0);
  total = $state<number | null>(null);

  get percent(): number {
    if (!this.total || this.total <= 0) return 0;
    return Math.min(100, (this.downloaded / this.total) * 100);
  }

  /**
   * Consulta el feed del updater. Si hay versión nueva abre el diálogo, tanto
   * en el arranque como al pulsar el botón manual.
   */
  async check(): Promise<NebulaUpdateStatus | null> {
    if (this.checking) return this.status;
    this.checking = true;
    this.error = "";
    try {
      const result = await api.checkAndPrepareUpdate();
      this.status = result;
      if (result.available) this.open = true;
      return result;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      this.checking = false;
    }
  }

  async install() {
    if (!this.status?.available || this.installing) return;
    this.installing = true;
    this.error = "";
    this.downloaded = 0;
    this.total = null;
    try {
      await api.installUpdate();
      // Lo normal es que la app se reinicie aquí; si no, no dejar el botón colgado.
      this.installing = false;
      this.open = false;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      this.installing = false;
    }
  }

  close() {
    if (this.installing) return;
    this.open = false;
  }
}

export const updates = new UpdatesState();
