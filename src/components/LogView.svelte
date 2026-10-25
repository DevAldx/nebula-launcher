<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { tick } from "svelte";

  let { id }: { id: string } = $props();
  const lines = $derived(app.logs[id] ?? []);
  let box = $state<HTMLDivElement | null>(null);

  // Autoscroll solo si el usuario ya estaba al final.
  $effect(() => {
    lines.length;
    if (!box) return;
    const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 40;
    if (atBottom) void tick().then(() => box && (box.scrollTop = box.scrollHeight));
  });
</script>

<div class="logs mono" bind:this={box}>
  {#if lines.length === 0}
    <span class="muted">Los logs de Minecraft aparecerán aquí al jugar.</span>
  {/if}
  {#each lines as l}
    <div class:err={l.stream === "stderr"}>{l.text}</div>
  {/each}
</div>

<style>
  .logs { height: 220px; overflow: auto; font-size: 12px; line-height: 1.5; background: rgb(0 0 0 / 0.35);
    border: 1px solid var(--border); border-radius: 9px; padding: 10px 12px; white-space: pre-wrap; word-break: break-all; user-select: text; }
  .err { color: #ffb4a8; }
</style>
