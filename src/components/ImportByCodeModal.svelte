<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api } from "../lib/api";
  import type { InstanceManifest } from "../lib/types";

  let { onclose }: { onclose: () => void } = $props();

  let code = $state("");
  let manifest = $state<InstanceManifest | null>(null);
  let looking = $state(false);
  let lookupError = $state("");

  async function lookup() {
    lookupError = "";
    manifest = null;
    if (!code.trim()) return;
    looking = true;
    try {
      manifest = await api.fetchShareManifest(code.trim());
    } catch (e) {
      lookupError = e instanceof Error ? e.message : String(e);
    } finally {
      looking = false;
    }
  }

  async function install() {
    if (!manifest) return;
    await app.importFromCode(manifest);
  }

  const progress = $derived(manifest ? app.progress[manifest.code] : undefined);
  const pct = $derived(
    progress && progress.total > 0 ? Math.min(100, Math.round((progress.done / progress.total) * 100)) : null,
  );
  const fileCount = $derived(
    manifest
      ? manifest.mods.length + manifest.resourcepacks.length + manifest.shaderpacks.length + manifest.config_files.length
      : 0,
  );
  const javaOk = $derived(
    !manifest?.java || app.java.some((j) => j.major >= manifest!.java!.min_major),
  );
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="card modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1>Importar por código</h1>
    <br />

    {#if app.importedId}
      <p class="done">¡Instancia instalada!</p>
      <p class="muted">Ya la tienes disponible en Instancias.</p>
      <br />
      <div class="actions"><span class="spacer"></span><button class="btn primary" onclick={onclose}>Cerrar</button></div>
    {:else}
      <label class="field">
        <span>Código</span>
        <div class="row">
          <input bind:value={code} placeholder="NBL-K7P2X9" class="mono" onkeydown={(e) => e.key === "Enter" && lookup()} />
          <button class="btn" onclick={lookup} disabled={looking || !code.trim()}>
            {looking ? "Buscando…" : "Buscar"}
          </button>
        </div>
      </label>
      {#if lookupError}<p class="error">{lookupError}</p>{/if}

      {#if manifest}
        <br />
        <div class="preview">
          <strong>{manifest.name}</strong>
          {#if manifest.description}<p class="muted">{manifest.description}</p>{/if}
          <p class="muted">
            Minecraft {manifest.minecraft_version} ·
            {manifest.loader === "fabric" ? `Fabric ${manifest.loader_version ?? ""}`.trim() : "Vanilla"}
            · {fileCount} archivos
          </p>
          {#if manifest.java && !javaOk}
            <p class="hint">Requiere Java {manifest.java.min_major}+; se instalará automáticamente al importar.</p>
          {/if}
          {#if manifest.background_url}
            <p class="hint">También se descargará el fondo de la instancia.</p>
          {/if}
        </div>
        <br />
        {#if app.importBusy}
          <div class="bar"><div class="fill" style:width={pct === null ? "35%" : `${pct}%`} class:indeterminate={pct === null}></div></div>
          <p class="hint">{progress?.phase ?? "Preparando…"}{pct !== null ? ` · ${pct}%` : ""}</p>
        {:else}
          <button class="btn primary" onclick={install}>Instalar</button>
        {/if}
        {#if app.importError}<p class="error">{app.importError}</p>{/if}
      {/if}

      <br />
      <div class="actions">
        <span class="spacer"></span>
        <button class="btn" onclick={onclose} disabled={app.importBusy}>Cancelar</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(0 0 0 / 0.55); display: grid; place-items: center; }
  .modal { width: 440px; max-width: 92vw; background: rgb(var(--surface) / 1); }
  .row { display: flex; gap: 8px; }
  .row input { flex: 1; }
  .preview { background: rgb(0 0 0 / 0.25); border: 1px solid var(--border); border-radius: 9px; padding: 12px 14px; }
  .actions { display: flex; }
  .spacer { flex: 1; }
  .done { color: var(--accent); font-weight: 650; font-size: 18px; }
  .bar { height: 8px; border-radius: 6px; background: rgb(255 255 255 / 0.08); overflow: hidden; }
  .fill { height: 100%; background: var(--accent); border-radius: 6px; transition: width var(--ease); }
  .fill.indeterminate { animation: slide 1.1s ease-in-out infinite alternate; }
  @keyframes slide { from { margin-left: 0; } to { margin-left: 65%; } }
</style>
