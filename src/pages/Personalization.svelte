<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api } from "../lib/api";

  let selectedId = $state(app.settings?.selected_instance ?? app.list.instances[0]?.id ?? "");
  let fileInput = $state<HTMLInputElement | null>(null);
  let busy = $state(false);
  let error = $state("");

  const instance = $derived(app.list.instances.find((i) => i.id === selectedId) ?? null);

  $effect(() => {
    if (selectedId) void app.ensureBackground(selectedId);
  });

  const bg = $derived(selectedId ? app.backgrounds[selectedId] : undefined);

  const MAX_BYTES = 15 * 1024 * 1024;
  const ALLOWED = ["image/png", "image/jpeg", "image/webp"];

  function readAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ""));
      reader.onerror = () => reject(reader.error ?? new Error("no se pudo leer el archivo"));
      reader.readAsDataURL(file);
    });
  }

  async function onFileChosen(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !selectedId) return;
    error = "";
    if (!ALLOWED.includes(file.type)) {
      error = "Usa una imagen PNG, JPG o WEBP.";
      return;
    }
    if (file.size > MAX_BYTES) {
      error = "La imagen supera los 15 MB.";
      return;
    }
    busy = true;
    try {
      const dataUrl = await readAsDataUrl(file);
      const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
      await api.setInstanceBackground(selectedId, file.type, base64);
      app.invalidateBackground(selectedId);
      await app.ensureBackground(selectedId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function clearBackground() {
    if (!selectedId) return;
    busy = true;
    error = "";
    try {
      await api.clearInstanceBackground(selectedId);
      app.invalidateBackground(selectedId);
      await app.ensureBackground(selectedId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="head">
  <div class="head-text">
    <h1>Personalizar</h1>
    <p class="muted">Asigna un fondo a cada instancia para darle su propia identidad visual.</p>
  </div>
</div>

{#if app.list.instances.length === 0}
  <div class="card empty">
    <h2>Sin instancias</h2>
    <p class="muted">Crea una instancia primero para poder personalizarla.</p>
  </div>
{:else}
  <div class="card controls">
    <label class="field">
      <span>Instancia</span>
      <select bind:value={selectedId}>
        {#each app.list.instances as inst (inst.id)}
          <option value={inst.id}>{inst.name}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="preview" class:nebula-bg={!bg} style:background-image={bg ? `url(${bg})` : undefined}>
    <div class="preview-overlay"></div>
    <div class="overlay">
      {#if instance}
        <span class="pill">{instance.id}</span>
        <strong class="preview-name">{instance.name}</strong>
        <span class="muted">Minecraft {instance.minecraft_version}</span>
      {/if}
    </div>
  </div>

  <div class="actions">
    <input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/webp" hidden onchange={onFileChosen} />
    <button class="btn primary" onclick={() => fileInput?.click()} disabled={busy}>
      {busy ? "Guardando…" : "Elegir imagen de fondo…"}
    </button>
    {#if bg}
      <button class="btn" onclick={clearBackground} disabled={busy}>Quitar fondo</button>
    {/if}
  </div>
  {#if error}<p class="error">{error}</p>{/if}
  <p class="hint">PNG, JPG o WEBP, hasta 15 MB. Se muestra en la pantalla principal y en la biblioteca de instancias.</p>
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }
  .head-text {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .empty {
    text-align: center;
    align-items: center;
  }
  .controls {
    padding: clamp(16px, 4vw, 20px);
  }
  .field {
    margin-bottom: 0;
  }
  .preview {
    position: relative;
    height: clamp(240px, 34vw, 320px);
    border-radius: var(--radius-lg);
    background-size: cover;
    background-position: center;
    border: 1px solid var(--border);
    display: flex;
    align-items: flex-end;
    overflow: hidden;
    box-shadow: 0 80px 110px -90px var(--accent-glow);
  }
  .preview-overlay {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(circle at top, rgb(8 7 16 / 0.2), transparent 55%),
      linear-gradient(to top, rgb(7 6 15 / 0.98) 12%, rgb(7 6 15 / 0.6) 40%, transparent 80%);
  }
  .preview > * {
    position: relative;
    z-index: 1;
  }
  .overlay {
    width: 100%;
    padding: clamp(18px, 6vw, 30px);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pill {
    align-self: flex-start;
    padding: 5px 12px;
    border-radius: 999px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    background: rgb(0 0 0 / 0.5);
    border: 1px solid rgb(255 255 255 / 0.14);
    color: var(--muted);
  }
  .preview-name {
    font-size: clamp(28px, 7vw, 48px);
    font-weight: 650;
    letter-spacing: -0.05em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
</style>
