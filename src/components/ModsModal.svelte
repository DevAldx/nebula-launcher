<script lang="ts">
  import { api } from "../lib/api";
  import type { Instance, ModEntry } from "../lib/types";

  let { instance, onclose }: { instance: Instance; onclose: () => void } = $props();

  let mods = $state<ModEntry[]>([]);
  let loading = $state(true);
  let error = $state("");
  let busy = $state<string | null>(null); // file_name en curso, para deshabilitar sus botones
  let fileInput = $state<HTMLInputElement | null>(null);

  async function refresh() {
    loading = true;
    error = "";
    try {
      mods = await api.listMods(instance.id);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }
  refresh();

  function displayName(fileName: string): string {
    return fileName.endsWith(".disabled") ? fileName.slice(0, -".disabled".length) : fileName;
  }

  function formatSize(bytes: number): string {
    return bytes >= 1024 * 1024 ? `${(bytes / (1024 * 1024)).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
  }

  async function toggle(mod: ModEntry) {
    busy = mod.file_name;
    error = "";
    try {
      await api.setModEnabled(instance.id, mod.file_name, !mod.enabled);
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  async function remove(mod: ModEntry) {
    busy = mod.file_name;
    error = "";
    try {
      await api.deleteMod(instance.id, mod.file_name);
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  function readAsBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = String(reader.result ?? "");
        resolve(result.slice(result.indexOf(",") + 1));
      };
      reader.onerror = () => reject(reader.error ?? new Error("no se pudo leer el archivo"));
      reader.readAsDataURL(file);
    });
  }

  async function onFilesChosen(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const chosen = Array.from(input.files ?? []);
    input.value = ""; // permite volver a elegir el mismo archivo más tarde
    if (chosen.length === 0) return;
    busy = "__import__";
    error = "";
    try {
      const files = await Promise.all(
        chosen.map(async (f) => ({ name: f.name, data_base64: await readAsBase64(f) })),
      );
      await api.importMods(instance.id, files);
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  async function openFolder(sub: "mods" | "config") {
    error = "";
    try {
      await api.openInstanceFolder(instance.id, sub);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="card modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1>Mods de {instance.name}</h1>
    <p class="muted mono">{instance.id}</p>
    <br />

    <div class="toolbar">
      <input
        bind:this={fileInput}
        type="file"
        accept=".jar"
        multiple
        hidden
        onchange={onFilesChosen}
      />
      <button class="btn primary" onclick={() => fileInput?.click()} disabled={busy === "__import__"}>
        {busy === "__import__" ? "Importando…" : "+ Importar mods"}
      </button>
      <button class="btn" onclick={() => openFolder("mods")}>Abrir carpeta de mods</button>
      <button class="btn" onclick={() => openFolder("config")}>Abrir carpeta de config</button>
    </div>
    <br />

    {#if error}<p class="error">{error}</p>{/if}

    {#if loading}
      <p class="muted">Cargando…</p>
    {:else if mods.length === 0}
      <p class="muted">Sin mods todavía. Importa un .jar o arrástralo a la carpeta de mods.</p>
    {:else}
      <div class="list">
        {#each mods as mod (mod.file_name)}
          <div class="row">
            <label class="check">
              <input
                type="checkbox"
                checked={mod.enabled}
                disabled={busy === mod.file_name}
                onchange={() => toggle(mod)}
              />
              <span class:disabled={!mod.enabled}>{displayName(mod.file_name)}</span>
            </label>
            <span class="muted size">{formatSize(mod.size_bytes)}</span>
            <button class="btn icon danger" title="Eliminar" disabled={busy === mod.file_name} onclick={() => remove(mod)}>✕</button>
          </div>
        {/each}
      </div>
    {/if}

    <br />
    <div class="actions">
      <span class="spacer"></span>
      <button class="btn" onclick={onclose}>Cerrar</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(0 0 0 / 0.55); display: grid; place-items: center; }
  .modal { width: 520px; max-width: 92vw; max-height: 80vh; overflow-y: auto; background: rgb(var(--surface) / 1); }
  .toolbar { display: flex; gap: 10px; flex-wrap: wrap; }
  .list { display: flex; flex-direction: column; gap: 6px; max-height: 320px; overflow-y: auto; }
  .row { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 8px; }
  .check { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; cursor: pointer; }
  .check span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .check span.disabled { color: var(--muted); text-decoration: line-through; }
  .size { font-size: 12px; white-space: nowrap; }
  .actions { display: flex; }
  .spacer { flex: 1; }
</style>
