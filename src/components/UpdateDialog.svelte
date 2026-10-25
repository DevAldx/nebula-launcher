<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { updates } from "../lib/updates.svelte";

  type Progress = { downloaded: number; total: number | null };

  const mb = (bytes: number) => (bytes / (1024 * 1024)).toFixed(1);

  onMount(() => {
    // Comprobación silenciosa al arrancar, sin bloquear la primera pantalla.
    const t = setTimeout(() => void updates.check(), 2500);
    let unlisten: (() => void) | undefined;
    void listen<Progress>("update-progress", (e) => {
      updates.downloaded = e.payload.downloaded;
      updates.total = e.payload.total;
    }).then((fn) => (unlisten = fn));
    return () => {
      clearTimeout(t);
      unlisten?.();
    };
  });
</script>

{#if updates.open && updates.status?.update}
  <button class="backdrop" onclick={() => updates.close()} aria-label="Cerrar"></button>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="update-title">
    <div class="dialog-header">
      <div>
        <h2 id="update-title">Actualización disponible</h2>
        <p class="muted">Una nueva versión de Nebula Launcher está lista</p>
      </div>
      <button class="icon-btn" onclick={() => updates.close()} disabled={updates.installing} aria-label="Cerrar">✕</button>
    </div>
    <div class="dialog-body">
      <div class="version-row">
        <div>
          <span class="muted">Versión actual</span>
          <p class="version">{updates.status.update.current_version}</p>
        </div>
        <span class="arrow">→</span>
        <div>
          <span class="muted">Nueva versión</span>
          <p class="version new">{updates.status.update.version}</p>
        </div>
      </div>
      {#if updates.status.update.notes}
        <div class="notes">
          <h3>Novedades</h3>
          <p>{updates.status.update.notes}</p>
        </div>
      {/if}
      {#if updates.error}<p class="error">{updates.error}</p>{/if}
      {#if updates.installing}
        <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={updates.percent}>
          <div class="bar" style:width={`${updates.percent}%`}></div>
        </div>
        <p class="hint">
          {#if updates.total}
            Descargando {mb(updates.downloaded)} / {mb(updates.total)} MB ({updates.percent.toFixed(0)} %)
          {:else}
            Descargando actualización...
          {/if}
        </p>
      {/if}
    </div>
    <div class="dialog-footer">
      <button class="btn" onclick={() => updates.close()} disabled={updates.installing}>Más tarde</button>
      <button class="btn primary" onclick={() => updates.install()} disabled={updates.installing}>
        {updates.installing ? "Instalando..." : "Actualizar ahora"}
      </button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: none;
    background: rgb(0 0 0 / 0.7);
    backdrop-filter: blur(4px);
    z-index: 100;
    animation: fadeIn 200ms ease;
    cursor: default;
  }
  .dialog {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(480px, calc(100% - 32px));
    background: radial-gradient(circle at top, rgb(167 139 250 / 0.16), transparent 60%), rgb(var(--surface) / 0.98);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    z-index: 101;
    box-shadow: 0 100px 140px -80px var(--accent-glow);
    animation: slideUp 200ms ease;
    overflow: hidden;
  }
  .dialog-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 24px 26px 18px;
  }
  .dialog-header h2 {
    font-size: 18px;
    font-weight: 650;
    text-transform: none;
    letter-spacing: -0.01em;
    color: var(--text);
    margin: 0;
  }
  .dialog-body {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 0 26px 20px;
  }
  .version-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 18px;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.04);
    border: 1px solid var(--border);
  }
  .version {
    font-size: 18px;
    font-weight: 650;
    margin-top: 4px;
  }
  .version.new {
    color: var(--accent-hover);
  }
  .arrow {
    color: var(--muted);
    font-size: 18px;
  }
  .notes {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 16px 18px;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.03);
    border: 1px solid var(--border);
  }
  .notes h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--muted);
  }
  .notes p {
    white-space: pre-wrap;
    color: var(--text);
  }
  .progress {
    height: 8px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.08);
    overflow: hidden;
  }
  .bar {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--accent), var(--accent-hover));
    transition: width 200ms linear;
  }
  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 16px 26px 24px;
    border-top: 1px solid var(--border);
  }
  .icon-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 8px;
    width: 32px;
    height: 32px;
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.06);
  }
  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  @keyframes slideUp {
    from { opacity: 0; transform: translate(-50%, -48%); }
    to { opacity: 1; transform: translate(-50%, -50%); }
  }
</style>
