<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api } from "../lib/api";
  import PlayButton from "../components/PlayButton.svelte";
  import LogView from "../components/LogView.svelte";
  import InstanceModal from "../components/InstanceModal.svelte";
  import SkinFace from "../components/SkinFace.svelte";
  import Icon from "../components/Icon.svelte";
  import { lazyBackground } from "../lib/lazyBackground";
  import type { Instance } from "../lib/types";

  let { goto }: { goto: (p: "instances" | "personalization") => void } = $props();
  const inst = $derived(app.selected);
  let editing = $state(false);

  const javaLabel = (i: Instance) => {
    const java = i.java;
    if (java.kind === "auto") return "Java automático";
    return `Java ${app.java.find((j) => j.path === java.path)?.major ?? "?"}`;
  };
  const loaderLabel = (i: Instance) =>
    i.loader === "fabric" ? `Fabric ${i.loader_version ?? ""}`.trim() : "Vanilla";

  function formatLastPlayed(ts: number | null): string {
    if (!ts) return "Nunca jugada";
    const days = Math.floor((Date.now() / 1000 - ts) / 86400);
    if (days <= 0) return "Última sesión: hoy";
    if (days === 1) return "Última sesión: ayer";
    if (days < 30) return `Última sesión: hace ${days} días`;
    return `Última sesión: ${new Date(ts * 1000).toLocaleDateString()}`;
  }

  function formatPlaytime(secs: number): string {
    if (secs < 60) return "Sin tiempo jugado todavía";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return h > 0 ? `${h} h ${m} min jugadas` : `${m} min jugados`;
  }

  // Más recientes primero (nunca jugadas al final), para la franja de cambio rápido.
  const recent = $derived(
    [...app.list.instances].sort((a, b) => (b.last_played ?? 0) - (a.last_played ?? 0)).slice(0, 8),
  );

  // Solo el fondo de la instancia protagonista se pide de inmediato; el resto
  // de la franja se carga al acercarse (ver `lazyBackground`).
  $effect(() => {
    if (inst) void app.ensureBackground(inst.id);
  });

  async function openFolder() {
    if (!inst) return;
    try {
      await api.openInstanceFolder(inst.id, ""); // raíz de la instancia (instance.json, mods, saves…)
    } catch {
      // silencioso: el botón de abajo ya muestra errores de otras acciones
    }
  }
</script>

<section class="card account">
  {#if app.account}
    <SkinFace url={app.account.skin_url} username={app.account.username} />
    <div class="who">
      <strong>{app.account.username}</strong>
      {#if app.account.kind === "offline"}
        <span class="muted">Cuenta local · servidores con online-mode=false</span>
      {:else}
        <span class="muted">Sesión de Microsoft activa</span>
      {/if}
    </div>
    <span class="spacer"></span>
    {#if app.account.kind === "offline"}
      <button class="btn" onclick={() => app.openLocalSignIn()}>Cambiar nombre</button>
    {/if}
    <button class="btn" onclick={() => app.signOut()}>
      <Icon name="logout" size={16} /> Cerrar sesión
    </button>
  {:else}
    <div class="who">
      <strong>Sin cuenta</strong>
      <span class="muted">
        Inicia sesión local (sin Microsoft) o con tu cuenta de Microsoft. El botón de Microsoft
        usa el flujo heredado, así que no necesitas Azure.
      </span>
    </div>
    <span class="spacer"></span>
    <button class="btn primary" onclick={() => app.openLocalSignIn()}>
      <Icon name="user" size={16} /> Iniciar sesión local
    </button>
    <button
      class="btn"
      title="Flujo heredado de Microsoft, sin Azure (no oficial)"
      onclick={() => app.startLegacySignIn()}
      disabled={app.signInBusy}
    >
      {app.signInBusy && app.signInLegacy ? "Esperando a Microsoft…" : "Microsoft"}
    </button>
  {/if}
</section>
{#if app.signInError}<p class="error inline">{app.signInError}</p>{/if}

{#if inst}
  {@const bg = app.backgrounds[inst.id]}
  <section class="hero" class:nebula-bg={!bg} style:background-image={bg ? `url(${bg})` : undefined}>
    <div class="hero-overlay"></div>
    <div class="top">
      <div class="status-group">
        <span
          class="badge"
          class:ready={inst.installed}
          title={inst.installed
            ? "Minecraft y Java ya están descargados en este PC."
            : "Los mods y ajustes de la instancia ya están aquí; Minecraft, librerías y Java se descargan en este PC al pulsar JUGAR."}
        >{inst.installed ? "Lista para jugar" : "Se descargará al jugar"}</span>
        <span class="badge subtle">{inst.id}</span>
      </div>
      <span class="spacer"></span>
      <button class="icon-btn" title="Editar instancia" onclick={() => (editing = true)}><Icon name="edit" size={18} /></button>
      <button class="icon-btn" title="Personalizar fondo" onclick={() => goto("personalization")}><Icon name="palette" size={18} /></button>
      <button class="icon-btn" title="Abrir carpeta de la instancia" onclick={openFolder}><Icon name="folder" size={18} /></button>
    </div>
    <div class="bottom">
      <div class="hero-meta">
        <p class="meta-line">Minecraft {inst.minecraft_version} · {loaderLabel(inst)} · {inst.mod_count} mods · {javaLabel(inst)}</p>
        <p class="meta-line muted">{formatLastPlayed(inst.last_played)} · {formatPlaytime(inst.total_playtime_secs)}</p>
      </div>
      <p class="name">{inst.name}</p>
      <div class="play-row">
        <div class="play"><PlayButton id={inst.id} big /></div>
      </div>
      {#if app.errors[inst.id]}<p class="error">{app.errors[inst.id]}</p>{/if}
    </div>
  </section>
{:else}
  <section class="card hero-empty">
    <div class="empty-content">
      <h2>Sin instancia seleccionada</h2>
      <p class="muted">Crea o importa una instancia para empezar a jugar.</p>
      <button class="btn primary" onclick={() => goto("instances")}>Ir a Instancias</button>
    </div>
  </section>
{/if}

{#if recent.length > 0}
  <div class="quickswitch-header">
    <span class="muted">Cambiar instancia</span>
  </div>
  <div class="quickswitch">
    {#each recent as r (r.id)}
      {@const rbg = app.backgrounds[r.id]}
      <button
        class="chip"
        class:nebula-bg={!rbg}
        class:active={r.id === inst?.id}
        style:background-image={rbg ? `url(${rbg})` : undefined}
        onclick={() => app.select(r.id)}
        title={r.name}
        use:lazyBackground={r.id}
      >
        <span class="chip-label">{r.name}</span>
      </button>
    {/each}
    <button class="chip add" onclick={() => goto("instances")} title="Añadir instancia">+</button>
  </div>
{/if}

{#if inst}
  <section class="card logs-card">
    <div class="logs-header">
      <h2>Registros</h2>
      <span class="muted">Salida en tiempo real</span>
    </div>
    <LogView id={inst.id} />
  </section>
{/if}

{#if editing && inst}
  <InstanceModal instance={inst} onclose={() => (editing = false)} />
{/if}

<style>
  .account {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    padding: 14px 20px;
  }
  .who {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .spacer {
    flex: 1;
  }
  .inline {
    margin-top: -10px;
  }

  .hero {
    position: relative;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    overflow: hidden;
    background-size: cover;
    background-position: center;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    min-height: clamp(320px, 38vw, 420px);
    box-shadow: 0 80px 120px -90px var(--accent-glow);
  }
  .hero-overlay {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(circle at top, rgb(8 7 16 / 0.25), transparent 55%),
      linear-gradient(to top, rgb(7 6 15 / 0.98) 12%, rgb(7 6 15 / 0.7) 38%, rgb(7 6 15 / 0.28) 72%, transparent 100%);
    pointer-events: none;
  }
  .hero > * {
    position: relative;
    z-index: 1;
  }
  .hero-empty {
    padding: clamp(24px, 6vw, 40px);
    text-align: center;
  }
  .empty-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: clamp(16px, 4vw, 24px) clamp(18px, 6vw, 36px);
  }
  .status-group {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.24em;
    padding: 6px 14px;
    border-radius: 999px;
    background: rgb(0 0 0 / 0.5);
    color: var(--muted);
    border: 1px solid rgb(255 255 255 / 0.12);
    backdrop-filter: blur(10px);
  }
  .badge.ready {
    color: #8ef0b0;
    border-color: rgb(142 240 176 / 0.4);
  }
  .badge.subtle {
    letter-spacing: 0.18em;
  }
  .icon-btn {
    background: rgb(0 0 0 / 0.5);
    border: 1px solid rgb(255 255 255 / 0.14);
    border-radius: 999px;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    cursor: pointer;
    backdrop-filter: blur(10px);
    transition: background var(--ease), transform var(--ease), border-color var(--ease);
  }
  .icon-btn:hover {
    background: rgb(0 0 0 / 0.7);
    border-color: rgb(255 255 255 / 0.28);
  }
  .icon-btn:active {
    transform: scale(0.96);
  }
  .bottom {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: clamp(18px, 6vw, 40px) clamp(18px, 6vw, 40px);
  }
  .hero-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .meta-line {
    font-size: 13px;
    color: rgb(236 235 245 / 0.9);
  }
  .name {
    font-size: clamp(34px, 9vw, 72px);
    font-weight: 650;
    letter-spacing: -0.06em;
    line-height: 0.98;
  }
  .play-row {
    margin-top: clamp(12px, 4vw, 22px);
  }
  .play {
    display: inline-flex;
  }

  .quickswitch-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    text-transform: uppercase;
    letter-spacing: 0.26em;
    font-size: 10px;
  }
  .quickswitch {
    display: flex;
    gap: 12px;
    overflow-x: auto;
    padding-bottom: 6px;
    scroll-snap-type: x mandatory;
  }
  .quickswitch::-webkit-scrollbar {
    height: 6px;
  }
  .quickswitch::-webkit-scrollbar-thumb {
    background: rgb(255 255 255 / 0.12);
    border-radius: 999px;
  }
  .chip {
    position: relative;
    flex: 0 0 auto;
    width: clamp(150px, 16vw, 190px);
    height: clamp(84px, 10vw, 108px);
    border-radius: 14px;
    border: 1px solid var(--border);
    background-size: cover;
    background-position: center;
    cursor: pointer;
    display: flex;
    align-items: flex-end;
    padding: 0;
    overflow: hidden;
    scroll-snap-align: start;
    transition: transform var(--ease), border-color var(--ease), box-shadow var(--ease);
    box-shadow: 0 60px 80px -70px transparent;
  }
  .chip::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to top, rgb(7 6 15 / 0.9) 12%, transparent 70%);
  }
  .chip:hover {
    transform: translateY(-4px);
  }
  .chip.active {
    border-color: var(--border-strong);
    box-shadow: 0 50px 80px -60px var(--accent-glow);
  }
  .chip-label {
    position: relative;
    z-index: 1;
    width: 100%;
    font-size: 12px;
    padding: 10px 12px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chip.add {
    display: grid;
    place-items: center;
    font-size: 26px;
    color: var(--muted);
    background: radial-gradient(circle at top, rgb(167 139 250 / 0.18), transparent 70%), rgb(var(--surface-2));
  }

  .logs-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: clamp(18px, 4vw, 26px);
  }
  .logs-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
</style>
