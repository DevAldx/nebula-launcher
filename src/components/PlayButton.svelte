<script lang="ts">
  import { app } from "../lib/app.svelte";

  let { id, big = false }: { id: string; big?: boolean } = $props();

  const state = $derived(app.games[id]);
  const prog = $derived(app.progress[id]);
  const pct = $derived(prog && prog.total > 0 ? Math.min(100, Math.round((prog.done / prog.total) * 100)) : null);
</script>

{#if state === "running"}
  <button class="btn danger play-btn" class:big onclick={() => app.stop(id)}>
    <span class="play-label">DETENER</span>
  </button>
{:else if state === "preparing"}
  <div class="prep" class:big>
    <div class="bar"><div class="fill" style:width={pct === null ? "38%" : `${pct}%`} class:indeterminate={pct === null}></div></div>
    <span class="hint prep-hint">{prog?.phase ?? "Preparando…"}{pct !== null ? ` · ${pct}%` : ""}</span>
  </div>
{:else}
  <button class="btn primary play-btn" class:big onclick={() => app.play(id)}>
    <span class="play-label">JUGAR</span>
  </button>
{/if}

<style>
  .play-btn {
    position: relative;
    overflow: hidden;
    text-transform: uppercase;
  }
  .big {
    padding: 16px 56px;
    font-size: 16px;
    letter-spacing: 0.42em;
  }
  .play-label {
    position: relative;
    z-index: 1;
  }
  .play-btn.primary::after {
    content: "";
    position: absolute;
    inset: 0;
    background: radial-gradient(circle at top, rgb(255 255 255 / 0.18), transparent 60%);
    opacity: 0;
    transition: opacity var(--ease);
  }
  .play-btn.primary:hover::after {
    opacity: 1;
  }
  .prep {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 240px;
  }
  .prep.big {
    min-width: 340px;
  }
  .bar {
    height: 10px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.08);
    overflow: hidden;
    border: 1px solid rgb(255 255 255 / 0.12);
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent) 0%, var(--accent-hover) 100%);
    border-radius: inherit;
    transition: width var(--ease);
  }
  .fill.indeterminate {
    width: 40%;
    animation: slide 1.2s ease-in-out infinite alternate;
  }
  .prep-hint {
    text-transform: uppercase;
    letter-spacing: 0.26em;
    font-size: 10px;
  }
  @keyframes slide {
    from { margin-left: 0; }
    to { margin-left: 60%; }
  }
</style>
