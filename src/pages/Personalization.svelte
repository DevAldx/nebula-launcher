<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api } from "../lib/api";
  import { accentColors, normalizeHex } from "../lib/color";
  import type { Settings } from "../lib/types";

  let {
    initialTab = "launcher",
  }: { initialTab?: "launcher" | "instances" } = $props();

  type Tab = "launcher" | "instances";
  // Si se llega desde la barra lateral o desde Inicio, la pestaña pedida manda.
  // El efecto permite cambiar de pestaña sin remontar la página.
  // svelte-ignore state_referenced_locally
  let tab = $state<Tab>(initialTab);
  $effect(() => {
    tab = initialTab;
  });

  const DEFAULT_UI: Settings["ui"] = {
    theme: "dark",
    animations: true,
    transparency: true,
    accent: null,
    background_fit: "cover",
    background_blur: 0,
    background_dim: 40,
  };

  // Copia editable de la apariencia. El fondo y el icono se guardan en cuanto
  // se eligen; los colores y efectos se aplican con «Guardar apariencia».
  let uiDraft = $state<Settings["ui"]>({ ...(app.settings?.ui ?? DEFAULT_UI) });
  let appearanceError = $state("");
  let appearanceSaved = $state(false);
  let savingAppearance = $state(false);

  let iconInput = $state<HTMLInputElement | null>(null);
  let launcherBackgroundInput = $state<HTMLInputElement | null>(null);
  let iconBusy = $state(false);
  let backgroundBusy = $state(false);
  let iconError = $state("");
  let backgroundError = $state("");

  const MAX_LAUNCHER_BYTES = 15 * 1024 * 1024;
  const MAX_ICON_BYTES = 4 * 1024 * 1024;
  const BACKGROUND_TYPES = ["image/png", "image/jpeg", "image/webp", "image/gif"];

  const THEME_ACCENTS: Record<Settings["ui"]["theme"], string> = {
    dark: "#8b5cf6",
    black: "#e5e5ea",
    purple: "#b794ff",
  };
  const previewAccent = $derived(
    accentColors(uiDraft.accent ?? THEME_ACCENTS[uiDraft.theme]),
  );
  const previewStyle = $derived(
    previewAccent
      ? `--accent: ${previewAccent.accent}; --accent-hover: ${previewAccent.hover}; --accent-glow: ${previewAccent.glow};`
      : "",
  );
  const previewBackground = $derived(
    uiDraft.theme === "black" ? "#000" : uiDraft.theme === "purple" ? "#120b24" : "#07060f",
  );

  const accentPresets = [
    "#8b5cf6",
    "#38bdf8",
    "#22c55e",
    "#f59e0b",
    "#f0556a",
    "#ec4899",
  ];

  function mimeOf(file: File): string {
    if (file.type) return file.type;
    const ext = file.name.split(".").pop()?.toLowerCase();
    if (ext === "png") return "image/png";
    if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
    if (ext === "webp") return "image/webp";
    if (ext === "gif") return "image/gif";
    return "";
  }

  function readAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result ?? ""));
      reader.onerror = () => reject(reader.error ?? new Error("no se pudo leer el archivo"));
      reader.readAsDataURL(file);
    });
  }

  function base64Of(dataUrl: string): string {
    return dataUrl.slice(dataUrl.indexOf(",") + 1);
  }

  async function onIconChosen(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    iconError = "";
    if (mimeOf(file) !== "image/png") {
      iconError = "Usa un PNG cuadrado (por ejemplo, 512×512).";
      return;
    }
    if (file.size > MAX_ICON_BYTES) {
      iconError = "El icono supera los 4 MB.";
      return;
    }
    iconBusy = true;
    try {
      const dataUrl = await readAsDataUrl(file);
      await app.setLauncherIcon(base64Of(dataUrl));
    } catch (e) {
      iconError = e instanceof Error ? e.message : String(e);
    } finally {
      iconBusy = false;
    }
  }

  async function clearIcon() {
    iconBusy = true;
    iconError = "";
    try {
      await app.clearLauncherIcon();
    } catch (e) {
      iconError = e instanceof Error ? e.message : String(e);
    } finally {
      iconBusy = false;
    }
  }

  async function onLauncherBackgroundChosen(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    backgroundError = "";
    const mime = mimeOf(file);
    if (!BACKGROUND_TYPES.includes(mime)) {
      backgroundError = "Usa una imagen PNG, JPG, WEBP o GIF.";
      return;
    }
    if (file.size > MAX_LAUNCHER_BYTES) {
      backgroundError = "El fondo supera los 15 MB.";
      return;
    }
    backgroundBusy = true;
    try {
      const dataUrl = await readAsDataUrl(file);
      await app.setLauncherBackground(mime, base64Of(dataUrl));
    } catch (e) {
      backgroundError = e instanceof Error ? e.message : String(e);
    } finally {
      backgroundBusy = false;
    }
  }

  async function clearLauncherBackground() {
    backgroundBusy = true;
    backgroundError = "";
    try {
      await app.clearLauncherBackground();
    } catch (e) {
      backgroundError = e instanceof Error ? e.message : String(e);
    } finally {
      backgroundBusy = false;
    }
  }

  async function saveAppearance() {
    if (!app.settings) return;
    appearanceError = "";
    appearanceSaved = false;
    const rawAccent = uiDraft.accent?.trim() ?? "";
    const accent = rawAccent ? normalizeHex(rawAccent) : null;
    if (rawAccent && !accent) {
      appearanceError = "El color de acento debe tener el formato #RRGGBB.";
      return;
    }
    savingAppearance = true;
    try {
      const next = $state.snapshot(app.settings) as Settings;
      next.ui = { ...$state.snapshot(uiDraft), accent };
      await app.saveSettings(next);
      uiDraft.accent = accent;
      appearanceSaved = true;
    } catch (e) {
      appearanceError = e instanceof Error ? e.message : String(e);
    } finally {
      savingAppearance = false;
    }
  }

  function resetAppearance() {
    uiDraft = { ...DEFAULT_UI };
    appearanceError = "";
    appearanceSaved = false;
  }

  // ───────────────────────────── Fondos de instancia ─────────────────────────────

  let selectedId = $state(app.settings?.selected_instance ?? app.list.instances[0]?.id ?? "");
  let instanceFileInput = $state<HTMLInputElement | null>(null);
  let instanceBusy = $state(false);
  let instanceError = $state("");

  const instance = $derived(app.list.instances.find((i) => i.id === selectedId) ?? null);

  $effect(() => {
    if (selectedId) void app.ensureBackground(selectedId);
  });

  const bg = $derived(selectedId ? app.backgrounds[selectedId] : undefined);

  async function onInstanceFileChosen(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !selectedId) return;
    instanceError = "";
    const mime = mimeOf(file);
    if (!BACKGROUND_TYPES.includes(mime)) {
      instanceError = "Usa una imagen PNG, JPG, WEBP o GIF.";
      return;
    }
    if (file.size > MAX_LAUNCHER_BYTES) {
      instanceError = "La imagen supera los 15 MB.";
      return;
    }
    instanceBusy = true;
    try {
      const dataUrl = await readAsDataUrl(file);
      await api.setInstanceBackground(selectedId, mime, base64Of(dataUrl));
      app.invalidateBackground(selectedId);
      await app.ensureBackground(selectedId);
    } catch (e) {
      instanceError = e instanceof Error ? e.message : String(e);
    } finally {
      instanceBusy = false;
    }
  }

  async function clearInstanceBackground() {
    if (!selectedId) return;
    instanceBusy = true;
    instanceError = "";
    try {
      await api.clearInstanceBackground(selectedId);
      app.invalidateBackground(selectedId);
      await app.ensureBackground(selectedId);
    } catch (e) {
      instanceError = e instanceof Error ? e.message : String(e);
    } finally {
      instanceBusy = false;
    }
  }
</script>

<div class="head">
  <div class="head-text">
    <h1>Personalizar</h1>
    <p class="muted">
      {#if tab === "launcher"}
        Cambia el icono, los colores y el fondo del launcher. Las imágenes, GIF y WEBP animados se
        reproducen en el fondo.
      {:else}
        Asigna un fondo propio a cada instancia. Es independiente del fondo del launcher.
      {/if}
    </p>
  </div>
</div>

<div class="tabs" role="tablist">
  <button
    type="button"
    class:active={tab === "launcher"}
    onclick={() => (tab = "launcher")}
    role="tab"
    aria-selected={tab === "launcher"}
  >
    Launcher
  </button>
  <button
    type="button"
    class:active={tab === "instances"}
    onclick={() => (tab = "instances")}
    role="tab"
    aria-selected={tab === "instances"}
  >
    Instancias
  </button>
</div>

{#if tab === "launcher"}
  <div class="launcher-grid">
    <section class="card">
      <h2>Vista previa</h2>
      <div class="preview" style={previewStyle}>
        <div
          class="preview-image"
          class:nebula-bg={!app.launcherBackground}
          style:background-color={previewBackground}
          style:background-image={app.launcherBackground
            ? `url("${app.launcherBackground}")`
            : undefined}
          style:background-size={uiDraft.background_fit === "cover"
            ? "cover"
            : uiDraft.background_fit === "contain"
              ? "contain"
              : "auto"}
          style:background-repeat={uiDraft.background_fit === "tile" ? "repeat" : "no-repeat"}
          style:filter={uiDraft.background_blur > 0
            ? `blur(${uiDraft.background_blur}px)`
            : undefined}
        ></div>
        <div class="preview-shade" style:opacity={uiDraft.background_dim / 100}></div>
        <div class="preview-content">
          <div class="preview-brand">
            <img src={app.launcherIcon ?? "/nebula-icon.png"} alt="" />
            <span>Nebula</span>
          </div>
          <div class="preview-hero">
            <span class="preview-pill">Vista previa</span>
            <strong>Tu launcher, a tu manera</strong>
            <p>Icono, colores y fondo personalizados.</p>
            <span class="preview-button">Jugar</span>
          </div>
        </div>
      </div>
    </section>

    <section class="card">
      <h2>Identidad</h2>
      <div class="identity">
        <img class="identity-icon" src={app.launcherIcon ?? "/nebula-icon.png"} alt="" />
        <div class="identity-text">
          <strong>Icono del launcher</strong>
          <p class="hint">
            PNG cuadrado, hasta 4 MB. Se usa en la barra lateral y como icono de la ventana.
          </p>
          <div class="actions">
            <input
              bind:this={iconInput}
              type="file"
              accept="image/png,.png"
              hidden
              onchange={onIconChosen}
            />
            <button
              type="button"
              class="btn primary"
              onclick={() => iconInput?.click()}
              disabled={iconBusy}
            >
              {iconBusy ? "Guardando…" : "Elegir icono…"}
            </button>
            {#if app.launcherIcon}
              <button type="button" class="btn" onclick={clearIcon} disabled={iconBusy}>
                Restablecer
              </button>
            {/if}
          </div>
          {#if iconError}<p class="error">{iconError}</p>{/if}
        </div>
      </div>
    </section>
  </div>

  <section class="card">
    <h2>Colores e interfaz</h2>
    <div class="row">
      <label class="field">
        <span>Tema base</span>
        <select bind:value={uiDraft.theme}>
          <option value="dark">Oscuro</option>
          <option value="black">Negro</option>
          <option value="purple">Púrpura</option>
        </select>
      </label>
      <label class="field">
        <span>Color de acento</span>
        <div class="accent-row">
          <input
            class="color-input"
            type="color"
            value={uiDraft.accent ?? "#8b5cf6"}
            oninput={(e) => (uiDraft.accent = e.currentTarget.value)}
            aria-label="Elegir color de acento"
          />
          <input
            type="text"
            value={uiDraft.accent ?? ""}
            placeholder="#8b5cf6"
            oninput={(e) => {
              const value = e.currentTarget.value.trim();
              uiDraft.accent = value === "" ? null : value;
            }}
          />
          <button type="button" class="btn" onclick={() => (uiDraft.accent = null)}>
            Tema
          </button>
        </div>
      </label>
    </div>
    <div class="swatches" aria-label="Colores predefinidos">
      {#each accentPresets as color (color)}
        <button
          type="button"
          class="swatch"
          class:selected={normalizeHex(uiDraft.accent ?? "") === color}
          style:background={color}
          title={color}
          aria-label={`Usar ${color}`}
          onclick={() => (uiDraft.accent = color)}
        ></button>
      {/each}
    </div>
    <div class="checks">
      <label class="field check">
        <input type="checkbox" bind:checked={uiDraft.animations} />
        <span>Animaciones de la interfaz</span>
      </label>
      <label class="field check">
        <input type="checkbox" bind:checked={uiDraft.transparency} />
        <span>Transparencia de paneles</span>
      </label>
    </div>
  </section>

  <section class="card">
    <h2>Fondo del launcher</h2>
    <p class="hint">
      Es el fondo general de Nebula, no el de una instancia. Puedes usar una imagen fija o un GIF /
      WEBP animado; se reproduce tal cual.
    </p>
    <div class="actions">
      <input
        bind:this={launcherBackgroundInput}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/gif,.png,.jpg,.jpeg,.webp,.gif"
        hidden
        onchange={onLauncherBackgroundChosen}
      />
      <button
        type="button"
        class="btn primary"
        onclick={() => launcherBackgroundInput?.click()}
        disabled={backgroundBusy}
      >
        {backgroundBusy ? "Guardando…" : "Elegir fondo…"}
      </button>
      {#if app.launcherBackground}
        <button
          type="button"
          class="btn"
          onclick={clearLauncherBackground}
          disabled={backgroundBusy}
        >
          Quitar fondo
        </button>
      {/if}
    </div>
    {#if backgroundError}<p class="error">{backgroundError}</p>{/if}
    <div class="row controls-row">
      <label class="field">
        <span>Ajuste</span>
        <select bind:value={uiDraft.background_fit}>
          <option value="cover">Cubrir</option>
          <option value="contain">Contener</option>
          <option value="tile">Mosaico</option>
        </select>
      </label>
      <label class="field">
        <span>Desenfoque: {uiDraft.background_blur} px</span>
        <input type="range" min="0" max="40" step="1" bind:value={uiDraft.background_blur} />
      </label>
      <label class="field">
        <span>Oscurecer: {uiDraft.background_dim}%</span>
        <input type="range" min="0" max="90" step="1" bind:value={uiDraft.background_dim} />
      </label>
    </div>
    <p class="hint">
      PNG, JPG, WEBP o GIF, hasta 15 MB. Los GIF y WEBP animados funcionan sin configuración extra.
      El fondo y el icono se guardan al instante; los colores y efectos, con «Guardar apariencia».
    </p>
  </section>

  {#if appearanceError}<p class="error">{appearanceError}</p>{/if}
  <div class="save-row">
    <button type="button" class="btn primary" onclick={saveAppearance} disabled={savingAppearance}>
      {savingAppearance ? "Guardando…" : "Guardar apariencia"}
    </button>
    <button type="button" class="btn" onclick={resetAppearance} disabled={savingAppearance}>
      Restablecer colores
    </button>
    {#if appearanceSaved}<span class="hint">Guardado.</span>{/if}
  </div>
{:else}
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

    <div
      class="instance-preview"
      class:nebula-bg={!bg}
      style:background-image={bg ? `url(${bg})` : undefined}
    >
      <div class="instance-preview-overlay"></div>
      <div class="overlay">
        {#if instance}
          <span class="pill">{instance.id}</span>
          <strong class="preview-name">{instance.name}</strong>
          <span class="muted">Minecraft {instance.minecraft_version}</span>
        {/if}
      </div>
    </div>

    <div class="actions">
      <input
        bind:this={instanceFileInput}
        type="file"
        accept="image/png,image/jpeg,image/webp,image/gif,.png,.jpg,.jpeg,.webp,.gif"
        hidden
        onchange={onInstanceFileChosen}
      />
      <button
        type="button"
        class="btn primary"
        onclick={() => instanceFileInput?.click()}
        disabled={instanceBusy}
      >
        {instanceBusy ? "Guardando…" : "Elegir imagen de fondo…"}
      </button>
      {#if bg}
        <button type="button" class="btn" onclick={clearInstanceBackground} disabled={instanceBusy}>
          Quitar fondo
        </button>
      {/if}
    </div>
    {#if instanceError}<p class="error">{instanceError}</p>{/if}
    <p class="hint">
      PNG, JPG, WEBP o GIF, hasta 15 MB. Se muestra en la pantalla principal y en la biblioteca de
      instancias.
    </p>
  {/if}
{/if}

<p class="legal-note">
  Nebula Launcher no está afiliado, asociado, autorizado, respaldado ni patrocinado por Mojang
  Studios, Microsoft Corporation ni sus filiales. Minecraft es una marca de Mojang Studios.
</p>

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
  .tabs {
    display: inline-flex;
    align-self: flex-start;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: rgb(var(--surface) / 0.6);
  }
  .tabs button {
    border: none;
    background: transparent;
    color: var(--muted);
    border-radius: 999px;
    padding: 7px 18px;
    cursor: pointer;
    transition: background var(--ease), color var(--ease);
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button.active {
    background: var(--accent);
    color: #0d0c16;
    font-weight: 650;
  }
  .launcher-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.45fr) minmax(260px, 0.8fr);
    gap: 18px;
  }
  .preview {
    position: relative;
    min-height: 260px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    overflow: hidden;
    display: flex;
    align-items: flex-end;
    background-color: var(--background);
  }
  .preview-image {
    position: absolute;
    inset: -28px;
    background-position: center;
    pointer-events: none;
  }
  .preview-shade {
    position: absolute;
    inset: 0;
    background: #05040c;
    pointer-events: none;
  }
  .preview-content {
    position: relative;
    z-index: 1;
    width: 100%;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 26px;
  }
  .preview-brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 12px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .preview-brand img {
    width: 32px;
    height: 32px;
    border-radius: 9px;
    object-fit: cover;
    border: 1px solid rgb(255 255 255 / 0.16);
  }
  .preview-hero {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .preview-hero strong {
    font-size: clamp(22px, 4vw, 32px);
    letter-spacing: -0.04em;
    line-height: 1.05;
  }
  .preview-hero p {
    color: var(--muted);
    font-size: 12px;
  }
  .preview-pill {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.2em;
    padding: 5px 10px;
    border-radius: 999px;
    background: rgb(0 0 0 / 0.45);
    border: 1px solid rgb(255 255 255 / 0.16);
    color: var(--muted);
  }
  .preview-button {
    margin-top: 4px;
    padding: 8px 20px;
    border-radius: 999px;
    background: var(--accent);
    color: #0d0c16;
    font-weight: 650;
    font-size: 13px;
    box-shadow: 0 20px 40px -24px var(--accent-glow);
  }
  .identity {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  .identity-icon {
    width: 76px;
    height: 76px;
    border-radius: 18px;
    object-fit: cover;
    border: 1px solid var(--border);
    background: rgb(0 0 0 / 0.3);
  }
  .identity-text {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .identity-text strong {
    font-size: 15px;
  }
  .accent-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .accent-row input[type="text"] {
    min-width: 110px;
    flex: 1;
  }
  .color-input {
    width: 42px;
    height: 36px;
    padding: 2px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: rgb(0 0 0 / 0.3);
    cursor: pointer;
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: -2px 0 14px;
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    box-shadow: 0 0 0 1px rgb(255 255 255 / 0.18);
    transition: transform var(--ease), border-color var(--ease);
  }
  .swatch:hover {
    transform: scale(1.1);
  }
  .swatch.selected {
    border-color: var(--text);
  }
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 24px;
  }
  .checks .field {
    margin-bottom: 0;
  }
  .controls-row {
    margin-top: 16px;
  }
  .controls-row .field {
    margin-bottom: 0;
  }
  .save-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
  }
  .legal-note {
    margin-top: 4px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    line-height: 1.5;
    color: var(--muted);
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
  .instance-preview {
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
  .instance-preview-overlay {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(circle at top, rgb(8 7 16 / 0.2), transparent 55%),
      linear-gradient(to top, rgb(7 6 15 / 0.98) 12%, rgb(7 6 15 / 0.6) 40%, transparent 80%);
  }
  .instance-preview > * {
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

  @media (max-width: 900px) {
    .launcher-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
