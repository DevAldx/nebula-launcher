<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { app, applyUi } from "./lib/app.svelte";
  import Home from "./pages/Home.svelte";
  import Instances from "./pages/Instances.svelte";
  import Personalization from "./pages/Personalization.svelte";
  import SettingsPage from "./pages/Settings.svelte";
  import SignInDialog from "./components/SignInDialog.svelte";
  import LocalSignInDialog from "./components/LocalSignInDialog.svelte";
  import UpdateDialog from "./components/UpdateDialog.svelte";
  import Icon, { type IconName } from "./components/Icon.svelte";

  type Page = "home" | "instances" | "personalization" | "settings";
  type PersonalizationTab = "launcher" | "instances";
  let page = $state<Page>("home");
  let personalizationTab = $state<PersonalizationTab>("launcher");

  function goTo(next: Page) {
    if (next === "personalization") personalizationTab = "launcher";
    page = next;
  }

  const nav: { id: Page; label: string; icon: IconName }[] = [
    { id: "home", label: "Inicio", icon: "home" },
    { id: "instances", label: "Instancias", icon: "library" },
    { id: "personalization", label: "Personalizar", icon: "palette" },
    { id: "settings", label: "Ajustes", icon: "settings" },
  ];

  onMount(() => app.init());
  $effect(() => {
    if (app.settings) applyUi(app.settings.ui);
  });
  const fadeMs = $derived(app.settings?.ui.animations === false ? 0 : 120);
  const ui = $derived(app.settings?.ui);
</script>

{#if app.launcherBackground && ui}
  <div
    class="launcher-bg"
    style:background-image={`url("${app.launcherBackground}")`}
    style:background-size={ui.background_fit === "cover"
      ? "cover"
      : ui.background_fit === "contain"
        ? "contain"
        : "auto"}
    style:background-repeat={ui.background_fit === "tile" ? "repeat" : "no-repeat"}
    style:filter={ui.background_blur > 0 ? `blur(${ui.background_blur}px)` : undefined}
  ></div>
  <div class="launcher-bg-shade" style:opacity={ui.background_dim / 100}></div>
{/if}

<div class="shell">
  <aside>
    <div class="brand" title="Nebula Launcher">
      <img class="brand-mark" src={app.launcherIcon ?? "/nebula-icon.png"} alt="" />
      <span class="brand-text">Nebula</span>
    </div>
    <nav>
      {#each nav as item}
        <button class:active={page === item.id} title={item.label} onclick={() => goTo(item.id)}>
          <span class="icon"><Icon name={item.icon} size={21} /></span>
          <span class="label">{item.label}</span>
        </button>
      {/each}
    </nav>
    <div class="aside-footer">
      <div class="version muted" title="Versión">v{app.info?.version ?? ""}</div>
      <div
        class="legal"
        title="Nebula Launcher no está afiliado, asociado ni respaldado por Mojang Studios ni Microsoft. Minecraft es una marca de Mojang Studios."
      >
        <span>No afiliado a Mojang</span>
      </div>
    </div>
  </aside>

  <main>
    {#if app.fatal}
      <div class="card state-card"><h1>No se pudo iniciar Nebula</h1><p class="error">{app.fatal}</p></div>
    {:else if !app.ready}
      <div class="state-card"><p class="muted">Cargando…</p></div>
    {:else}
      {#key page}
        <div class="page" in:fade={{ duration: fadeMs }}>
          {#if page === "home"}
            <Home
              goto={(p, tab) => {
                if (p === "personalization" && tab) personalizationTab = tab;
                page = p;
              }}
            />
          {:else if page === "instances"}
            <Instances />
          {:else if page === "personalization"}
            <Personalization initialTab={personalizationTab} />
          {:else}
            <SettingsPage />
          {/if}
        </div>
      {/key}
    {/if}
  </main>
</div>

<SignInDialog />
<LocalSignInDialog />
<UpdateDialog />

<style>
  .launcher-bg {
    position: fixed;
    inset: -64px;
    z-index: 0;
    background-position: center;
    background-color: var(--background);
    pointer-events: none;
    transform: scale(1.04);
    transform-origin: center;
  }
  .launcher-bg-shade {
    position: fixed;
    inset: 0;
    z-index: 0;
    background: #05040c;
    pointer-events: none;
  }
  .shell {
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: 88px 1fr;
    height: 100%;
    gap: 0;
  }
  aside {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 26px;
    padding: 22px 10px;
    background: radial-gradient(circle at top, rgb(167 139 250 / 0.14), transparent 60%), rgb(var(--surface) / var(--surface-alpha));
    border-right: 1px solid var(--border);
    backdrop-filter: blur(14px) saturate(120%);
    -webkit-backdrop-filter: blur(14px) saturate(120%);
  }
  .brand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    text-align: center;
  }
  .brand-mark {
    width: 42px;
    height: 42px;
    border-radius: 12px;
    object-fit: cover;
    border: 1px solid rgb(255 255 255 / 0.14);
    box-shadow: 0 30px 60px -40px var(--accent-glow);
  }
  .brand-text {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.28em;
    color: var(--muted);
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
    width: 100%;
  }
  nav button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    cursor: pointer;
    width: 100%;
    padding: 10px 6px;
    border-radius: 12px;
    color: var(--muted);
    transition: background var(--ease), color var(--ease), transform var(--ease);
    position: relative;
  }
  nav button .icon {
    font-size: 18px;
    line-height: 1;
  }
  nav button .label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.16em;
  }
  nav button:hover {
    background: rgb(255 255 255 / 0.06);
    color: var(--text);
  }
  nav button:active {
    transform: scale(0.98);
  }
  nav button.active {
    background: radial-gradient(circle, rgb(167 139 250 / 0.24), transparent 70%);
    color: var(--text);
  }
  nav button.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 18px;
    border-radius: 0 4px 4px 0;
    background: linear-gradient(to bottom, var(--accent-hover), var(--accent));
    box-shadow: 0 0 0 4px rgb(139 92 246 / 0.08);
  }
  .aside-footer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .version {
    font-size: 10px;
    letter-spacing: 0.2em;
    text-transform: uppercase;
  }
  .legal {
    max-width: 70px;
    text-align: center;
    font-size: 8px;
    line-height: 1.35;
    color: var(--muted);
    opacity: 0.72;
  }
  main {
    overflow-y: auto;
    height: 100%;
    position: relative;
  }
  .page {
    padding: clamp(18px, 4vw, 36px) clamp(18px, 6vw, 48px);
    min-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .state-card {
    margin: clamp(18px, 6vw, 48px);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
</style>
