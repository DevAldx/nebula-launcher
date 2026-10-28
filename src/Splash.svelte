<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "./lib/api";
  import { applyUi } from "./lib/app.svelte";
  import type { StartupProgress } from "./lib/types";

  const stepNames = ["Configuración", "Actualización", "Java", "Instancias", "Listo"];

  let progress = $state<StartupProgress>({
    step: 0,
    total: stepNames.length,
    percent: 4,
    label: "Iniciando Nebula",
    detail: "Preparando la ventana de bienvenida…",
    done: false,
    version: "",
  });
  let icon = $state("/nebula-icon.png");

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    void (async () => {
      // Primero escuchamos y después pedimos el estado: si el arranque ya
      // avanzó, `getStartupProgress` devuelve el último paso conocido.
      try {
        unlisten = await listen<StartupProgress>("startup-progress", (event) => {
          if (!disposed) progress = event.payload;
        });
        if (disposed) {
          unlisten();
          return;
        }
        const current = await api.getStartupProgress();
        if (!disposed && current.step >= progress.step) progress = current;
      } catch {
        // La animación sigue con el estado local; el fallback de Rust abrirá
        // la ventana principal aunque este comando falle.
      }

      try {
        const settings = await api.getSettings();
        if (!disposed) applyUi(settings.ui);
      } catch {
        // Tema predeterminado.
      }

      try {
        const custom = await api.getLauncherIcon();
        if (!disposed && custom) {
          icon = `data:${custom.mime};base64,${custom.data_base64}`;
        }
      } catch {
        // Icono predeterminado.
      }
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<div class="splash">
  <div class="glow glow-a" aria-hidden="true"></div>
  <div class="glow glow-b" aria-hidden="true"></div>
  <div class="stars" aria-hidden="true"></div>

  <div class="stage">
    <div class="mark-wrap">
      <div class="ring" aria-hidden="true"></div>
      <div class="orbit" aria-hidden="true"><span></span></div>
      <img class="mark" src={icon} alt="" />
    </div>

    <div class="titles">
      <h1>Nebula <span>Launcher</span></h1>
      <p class="tagline">Abriendo tu universo de juego</p>
    </div>

    <div class="status" aria-live="polite">
      <div class="status-line">
        <span class="label">{progress.label}</span>
        <span class="percent">{Math.round(progress.percent)}%</span>
      </div>
      <div
        class="track"
        role="progressbar"
        aria-label="Progreso de inicio"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={Math.round(progress.percent)}
      >
        <div class="fill" style:width={`${progress.percent}%`}></div>
      </div>
      <p class="detail">{progress.detail ?? "Preparando Nebula…"}</p>
    </div>

    <div class="steps" aria-hidden="true">
      {#each stepNames as name, index}
        <div
          class="step"
          class:done={progress.step > index + 1}
          class:current={progress.step === index + 1}
        >
          <span class="dot"></span>
          <span class="name">{name}</span>
        </div>
      {/each}
    </div>
  </div>

  <footer>
    <span>v{progress.version || "…"}</span>
    <span class="sep">·</span>
    <span>© 2026 Nebula Launcher</span>
    <span class="sep">·</span>
    <span>No afiliado a Mojang Studios</span>
  </footer>
</div>

<style>
  .splash {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    overflow: hidden;
    background:
      radial-gradient(circle at 50% 42%, rgb(139 92 246 / 0.16), transparent 42%),
      linear-gradient(160deg, #090716 0%, var(--background) 55%, #05040b 100%);
    color: var(--text);
    user-select: none;
  }

  .glow {
    position: absolute;
    width: 300px;
    height: 300px;
    border-radius: 50%;
    filter: blur(70px);
    pointer-events: none;
    opacity: 0.55;
  }
  .glow-a {
    top: -140px;
    left: -100px;
    background: rgb(139 92 246 / 0.5);
    animation: drift 9s ease-in-out infinite alternate;
  }
  .glow-b {
    right: -120px;
    bottom: -160px;
    background: rgb(56 189 248 / 0.35);
    animation: drift 11s ease-in-out infinite alternate-reverse;
  }

  .stars {
    position: absolute;
    inset: 0;
    pointer-events: none;
    opacity: 0.7;
    background-image:
      radial-gradient(1.4px 1.4px at 12% 22%, rgb(255 255 255 / 0.7) 50%, transparent 51%),
      radial-gradient(1px 1px at 26% 72%, rgb(255 255 255 / 0.5) 50%, transparent 51%),
      radial-gradient(1.6px 1.6px at 42% 14%, rgb(255 255 255 / 0.55) 50%, transparent 51%),
      radial-gradient(1px 1px at 58% 84%, rgb(255 255 255 / 0.45) 50%, transparent 51%),
      radial-gradient(1.2px 1.2px at 74% 28%, rgb(255 255 255 / 0.6) 50%, transparent 51%),
      radial-gradient(1px 1px at 88% 66%, rgb(255 255 255 / 0.5) 50%, transparent 51%),
      radial-gradient(1.4px 1.4px at 94% 12%, rgb(255 255 255 / 0.45) 50%, transparent 51%),
      radial-gradient(1px 1px at 8% 88%, rgb(255 255 255 / 0.4) 50%, transparent 51%);
    animation: twinkle 4.5s ease-in-out infinite alternate;
  }

  .stage {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    width: min(430px, calc(100% - 56px));
    animation: open-window 700ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }

  .mark-wrap {
    position: relative;
    display: grid;
    place-items: center;
    width: 84px;
    height: 84px;
    margin-bottom: 12px;
    animation: float 5s ease-in-out infinite;
  }
  .mark {
    position: relative;
    z-index: 2;
    width: 72px;
    height: 72px;
    border-radius: 20px;
    object-fit: cover;
    border: 1px solid rgb(255 255 255 / 0.18);
    box-shadow:
      0 0 0 6px rgb(139 92 246 / 0.08),
      0 24px 48px -18px var(--accent-glow);
  }
  .ring {
    position: absolute;
    inset: -6px;
    border-radius: 28px;
    padding: 1px;
    background: conic-gradient(
      from 0deg,
      transparent 0deg,
      var(--accent-hover) 90deg,
      transparent 180deg,
      transparent 360deg
    );
    -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
    -webkit-mask-composite: xor;
    mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
    mask-composite: exclude;
    animation: spin 4.5s linear infinite;
  }
  .orbit {
    position: absolute;
    inset: -14px;
    border-radius: 32px;
    animation: spin 7s linear infinite;
  }
  .orbit span {
    position: absolute;
    top: 50%;
    left: -2px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent-hover);
    box-shadow: 0 0 10px 2px var(--accent-glow);
  }

  .titles {
    text-align: center;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 0.28em;
    text-transform: uppercase;
    text-indent: 0.28em;
    line-height: 1.2;
  }
  h1 span {
    color: var(--accent-hover);
    font-weight: 400;
  }
  .tagline {
    margin-top: 5px;
    color: var(--muted);
    font-size: 10px;
    letter-spacing: 0.22em;
    text-transform: uppercase;
  }

  .status {
    width: 100%;
    margin-top: 22px;
  }
  .status-line {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 8px;
  }
  .label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .percent {
    font-size: 11px;
    color: var(--accent-hover);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.08em;
  }
  .track {
    position: relative;
    height: 5px;
    border-radius: 999px;
    overflow: hidden;
    background: rgb(255 255 255 / 0.08);
    border: 1px solid rgb(255 255 255 / 0.05);
  }
  .fill {
    position: relative;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--accent), var(--accent-hover));
    box-shadow: 0 0 16px var(--accent-glow);
    transition: width 650ms cubic-bezier(0.22, 0.8, 0.24, 1);
    overflow: hidden;
  }
  .fill::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, transparent, rgb(255 255 255 / 0.5), transparent);
    transform: translateX(-100%);
    animation: shimmer 1.7s ease-in-out infinite;
  }
  .detail {
    min-height: 17px;
    margin-top: 8px;
    color: var(--muted);
    font-size: 11px;
    line-height: 1.5;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .steps {
    display: flex;
    justify-content: space-between;
    width: 100%;
    margin-top: 18px;
    gap: 6px;
  }
  .step {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    opacity: 0.5;
    transition: opacity 300ms ease, color 300ms ease;
  }
  .step .dot {
    width: 100%;
    max-width: 56px;
    height: 3px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.12);
    transition: background 300ms ease, box-shadow 300ms ease;
  }
  .step .name {
    font-size: 8px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .step.current {
    color: var(--text);
    opacity: 1;
  }
  .step.current .dot {
    background: var(--accent-hover);
    box-shadow: 0 0 12px var(--accent-glow);
    animation: pulse 1.6s ease-in-out infinite;
  }
  .step.done {
    opacity: 0.9;
    color: var(--accent-hover);
  }
  .step.done .dot {
    background: var(--accent);
  }

  footer {
    position: absolute;
    bottom: 14px;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 7px;
    padding: 0 20px;
    color: var(--muted);
    font-size: 9px;
    letter-spacing: 0.08em;
    opacity: 0.75;
    z-index: 1;
  }
  footer .sep {
    opacity: 0.5;
  }

  @keyframes open-window {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.965);
      filter: blur(5px);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
      filter: blur(0);
    }
  }
  @keyframes float {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(-4px);
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.55;
    }
  }
  @keyframes shimmer {
    0% {
      transform: translateX(-100%);
    }
    60%,
    100% {
      transform: translateX(100%);
    }
  }
  @keyframes twinkle {
    from {
      opacity: 0.45;
    }
    to {
      opacity: 0.8;
    }
  }
  @keyframes drift {
    from {
      transform: translate3d(0, 0, 0) scale(1);
    }
    to {
      transform: translate3d(28px, 18px, 0) scale(1.12);
    }
  }
</style>
