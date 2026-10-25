<script lang="ts">
  import { app } from "../lib/app.svelte";
  import ImportByCodeModal from "../components/ImportByCodeModal.svelte";
  import InstanceModal from "../components/InstanceModal.svelte";
  import ModsModal from "../components/ModsModal.svelte";
  import PlayButton from "../components/PlayButton.svelte";
  import Icon from "../components/Icon.svelte";
  import { api } from "../lib/api";
  import { lazyBackground } from "../lib/lazyBackground";
  import type { Instance } from "../lib/types";

  let modal = $state<{ instance: Instance | null } | null>(null);
  let modsFor = $state<Instance | null>(null);
  let importing = $state(false);
  let exportedPath = $state<string | null>(null);
  let exportError = $state("");
  let folderError = $state("");

  async function openFolder(inst: Instance) {
    folderError = "";
    try {
      await api.openInstanceFolder(inst.id, "");
    } catch (e) {
      folderError = e instanceof Error ? e.message : String(e);
    }
  }

  async function exportInstance(inst: Instance) {
    exportError = "";
    exportedPath = null;
    try {
      exportedPath = await api.exportManifestTemplate(inst.id);
    } catch (e) {
      exportError = e instanceof Error ? e.message : String(e);
    }
  }

  function loaderLabel(inst: Instance): string {
    return inst.loader === "fabric" ? `Fabric ${inst.loader_version ?? ""}`.trim() : "Vanilla";
  }
</script>

<div class="head">
  <div class="head-text">
    <h1>Biblioteca de instancias</h1>
    <p class="muted">Gestiona, selecciona y personaliza tus instancias de Minecraft.</p>
  </div>
  <div class="head-actions">
    <button class="btn" onclick={() => (importing = true)}>Importar por código</button>
    <button class="btn primary" onclick={() => (modal = { instance: null })}>Crear instancia</button>
  </div>
</div>

<details class="card help">
  <summary>¿Cómo funcionan las instancias y por qué en otro PC pone «se descargará al jugar»?</summary>
  <div class="help-body">
    <p>
      Cada instancia es una carpeta independiente dentro de
      <span class="mono">{app.info?.data_dir}/instances/</span> (por ejemplo,
      <span class="mono">NBL-K7P2X9</span>). Dentro están <span class="mono">instance.json</span>
      (nombre, versión, RAM, Java), <span class="mono">mods</span>,
      <span class="mono">config</span>, <span class="mono">resourcepacks</span>,
      <span class="mono">shaderpacks</span> y <span class="mono">saves</span>.
    </p>
    <p>
      Minecraft, las librerías, los assets y Java son <strong>compartidos</strong> y viven fuera de
      la instancia. Si copias solo su carpeta a otro PC, la instancia aparece como
      «se descargará al jugar» porque faltan esos archivos comunes, no porque tengas que
      configurar nada: pulsa <strong>JUGAR</strong> y Nebula los descarga para ese PC.
    </p>
    <p>
      Para evitar la descarga, copia también las carpetas <span class="mono">versions</span>,
      <span class="mono">libraries</span>, <span class="mono">assets</span> y
      <span class="mono">java</span> del directorio de datos. El engranaje de cada tarjeta es
      opcional: solo cambia RAM, Java y argumentos; la versión y el loader quedan fijados al crear
      la instancia. Si la instancia guardó un Java manual de otro PC y esa ruta ya no existe,
      elige <strong>Automático</strong> para que Nebula descargue el JRE que pida la versión. El
      icono de carpeta abre la raíz de la instancia para copiarla.
    </p>
  </div>
</details>
{#if folderError}<p class="error">{folderError}</p>{/if}

{#if app.list.broken.length > 0}
  <div class="card warn">
    <h2>Instancias con errores</h2>
    {#each app.list.broken as b}
      <p class="error"><span class="mono">{b.id}</span>: {b.error}</p>
    {/each}
  </div>
{/if}

{#if app.list.instances.length === 0}
  <div class="card empty-state">
    <h2>Sin instancias</h2>
    <p class="muted">Crea la primera instancia con el botón "Crear instancia" o impórtala por código.</p>
  </div>
{:else}
  <div class="gallery">
    {#each app.list.instances as inst (inst.id)}
      {@const selected = app.settings?.selected_instance === inst.id}
      {@const thumbBg = app.backgrounds[inst.id]}
      <article class="instance-card" class:selected use:lazyBackground={inst.id}>
        <div class="thumb" class:nebula-bg={!thumbBg} style:background-image={thumbBg ? `url(${thumbBg})` : undefined}>
          <div class="thumb-overlay"></div>
          <div class="thumb-top">
            <span class="pill mono">{inst.id}</span>
            {#if selected}<span class="pill active">Seleccionada</span>{/if}
            <span class="spacer"></span>
            <span
              class="pill"
              title={inst.installed
                ? "Minecraft y Java ya están descargados en este PC."
                : "Los mods y ajustes ya están en la carpeta; Minecraft, librerías y Java se descargan al pulsar JUGAR."}
            >{inst.installed ? "Instalada" : "Se descargará al jugar"}</span>
          </div>
          <div class="thumb-bottom">
            <div>
              <p class="instance-name">{inst.name}</p>
              <p class="instance-meta">Minecraft {inst.minecraft_version} · {loaderLabel(inst)}</p>
            </div>
          </div>
        </div>
        <div class="card-body">
          <div class="meta-row">
            <span class="meta-pill">{inst.mod_count} mods</span>
            <span class="meta-pill">{inst.memory.max_mb} MB RAM</span>
            <span class="meta-pill">{loaderLabel(inst)}</span>
          </div>
          {#if app.errors[inst.id]}<p class="error">{app.errors[inst.id]}</p>{/if}
          <div class="actions">
            <PlayButton id={inst.id} />
            <button class="btn" disabled={selected} onclick={() => app.select(inst.id)}>
              {selected ? "Seleccionada" : "Seleccionar"}
            </button>
            <div class="actions-group">
              <button class="btn icon" title="Mods" onclick={() => (modsFor = inst)}><Icon name="puzzle" size={17} /></button>
              <button class="btn icon" title="Abrir carpeta de la instancia" onclick={() => openFolder(inst)}><Icon name="folder" size={17} /></button>
              <button class="btn icon" title="Exportar manifiesto" onclick={() => exportInstance(inst)}><Icon name="upload" size={17} /></button>
              <button class="btn icon" title="Ajustes de la instancia (opcional)" onclick={() => (modal = { instance: inst })}><Icon name="settings" size={17} /></button>
            </div>
          </div>
        </div>
      </article>
    {/each}
  </div>
{/if}

{#if exportedPath}
  <br />
  <p class="hint">Plantilla guardada en <span class="mono">{exportedPath}</span>. Súbela donde vayas a alojar tus manifiestos y rellena las URLs de cada mod.</p>
{/if}
{#if exportError}<p class="error">{exportError}</p>{/if}

{#if modal}
  <InstanceModal instance={modal.instance} onclose={() => (modal = null)} />
{/if}
{#if modsFor}
  <ModsModal instance={modsFor} onclose={() => (modsFor = null)} />
{/if}
{#if importing}
  <ImportByCodeModal onclose={() => (importing = false)} />
{/if}

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }
  .head-text {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .warn {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .help {
    padding: 0;
    overflow: hidden;
  }
  .help summary {
    cursor: pointer;
    padding: 16px 22px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }
  .help[open] summary {
    border-bottom: 1px solid var(--border);
    color: var(--text);
  }
  .help-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px 22px 20px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.55;
  }
  .help-body strong {
    color: var(--text);
  }
  .empty-state {
    display: flex;
    flex-direction: column;
    gap: 8px;
    text-align: center;
    align-items: center;
  }
  .gallery {
    display: grid;
    grid-template-columns: repeat(12, 1fr);
    gap: clamp(14px, 3vw, 18px);
  }
  .instance-card {
    grid-column: span 12;
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    overflow: hidden;
    background: radial-gradient(circle at top, rgb(167 139 250 / 0.12), transparent 60%), rgb(var(--surface) / var(--surface-alpha));
    box-shadow: 0 70px 100px -90px var(--accent-glow);
    transition: transform var(--ease), border-color var(--ease), box-shadow var(--ease);
    content-visibility: auto;
    contain-intrinsic-size: auto 380px;
  }
  .instance-card:hover {
    transform: translateY(-6px);
  }
  .instance-card.selected {
    border-color: var(--border-strong);
    box-shadow: 0 60px 100px -80px var(--accent-glow);
  }
  .thumb {
    position: relative;
    height: clamp(180px, 26vw, 240px);
    background-size: cover;
    background-position: center;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
  }
  .thumb-overlay {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(circle at top, rgb(8 7 16 / 0.2), transparent 55%),
      linear-gradient(to top, rgb(7 6 15 / 0.98) 10%, rgb(7 6 15 / 0.6) 40%, transparent 80%);
  }
  .thumb > * {
    position: relative;
    z-index: 1;
  }
  .thumb-top {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 20px;
    flex-wrap: wrap;
  }
  .thumb-bottom {
    display: flex;
    align-items: flex-end;
    padding: 0 20px 18px;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    padding: 5px 12px;
    border-radius: 999px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    background: rgb(0 0 0 / 0.5);
    border: 1px solid rgb(255 255 255 / 0.14);
    color: var(--muted);
  }
  .pill.active {
    color: var(--text);
    border-color: rgb(167 139 250 / 0.6);
  }
  .spacer {
    flex: 1;
  }
  .instance-name {
    font-size: clamp(26px, 6vw, 42px);
    font-weight: 650;
    letter-spacing: -0.05em;
  }
  .instance-meta {
    color: rgb(236 235 245 / 0.88);
  }
  .card-body {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 18px 22px 22px;
  }
  .meta-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .meta-pill {
    padding: 6px 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    font-size: 12px;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  .actions-group {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-left: auto;
  }
  @media (min-width: 1080px) {
    .instance-card {
      grid-column: span 6;
    }
  }
</style>
