<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api } from "../lib/api";

  const info = $derived(app.signIn);
  let openError = $state("");
  let copied = $state(false);

  async function open() {
    if (!info) return;
    openError = "";
    try {
      await api.openUrl(info.verificationUri);
    } catch (e) {
      openError = e instanceof Error ? e.message : String(e);
    }
  }

  async function copy() {
    if (!info) return;
    try {
      await navigator.clipboard.writeText(info.userCode);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // el usuario puede seleccionar el código a mano
    }
  }
</script>

{#if info}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => app.cancelSignIn()}>
    <div class="card modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <h1>Iniciar sesión con Microsoft</h1>
      {#if app.signInLegacy}
        <p class="hint">
          Método heredado (no oficial): usa el identificador público del launcher oficial de
          Minecraft y no necesita una app de Azure.
        </p>
      {/if}
      <br />
      <p class="muted">Abre este enlace y escribe el código:</p>
      <div class="code mono" onclick={copy} role="button" tabindex="0">{info.userCode}</div>
      {#if copied}<p class="hint">Copiado.</p>{/if}
      <br />
      <button class="btn primary" onclick={open}>Abrir en el navegador</button>
      {#if openError}<p class="error">{openError}</p>{/if}
      <p class="hint link mono">{info.verificationUri}</p>
      <br />
      <div class="actions">
        <span class="spacer"></span>
        <button class="btn" onclick={() => app.cancelSignIn()}>Cancelar</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 50;
    background: rgb(0 0 0 / 0.72);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    display: grid; place-items: center;
    padding: 20px; overflow: auto;
  }
  .modal { width: 420px; max-width: 92vw; background: rgb(var(--surface) / 1); text-align: center; }
  .code {
    font-size: 30px; letter-spacing: 0.12em; font-weight: 650; color: var(--accent);
    background: rgb(0 0 0 / 0.3); border: 1px solid var(--border); border-radius: 9px;
    padding: 14px; cursor: pointer; user-select: text;
  }
  .link { word-break: break-all; }
  .actions { display: flex; gap: 10px; }
  .spacer { flex: 1; }
</style>
