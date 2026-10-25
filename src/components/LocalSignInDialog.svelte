<script lang="ts">
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  let username = $state("");

  function submit(e: Event) {
    e.preventDefault();
    if (!username.trim()) return;
    void app.signInOffline(username.trim());
  }
</script>

{#if app.localSignInOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => app.closeLocalSignIn()}>
    <div class="card modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <form onsubmit={submit}>
        <div class="title">
          <span class="icon"><Icon name="user" size={22} /></span>
          <div>
            <h1>Cuenta local</h1>
            <p class="muted">Sin Microsoft, sin Azure y sin contraseña.</p>
          </div>
        </div>
        <br />
        <label class="field">
          <span>Nombre de jugador</span>
          <input
            bind:value={username}
            maxlength="16"
            placeholder="Steve"
            autocomplete="off"
            spellcheck="false"
            disabled={app.localSignInBusy}
          />
        </label>
        <p class="hint">
          De 3 a 16 caracteres: letras, números y <span class="mono">_</span>. Se guarda en tu equipo
          y el UUID se genera solo, siempre igual para ese nombre.
        </p>
        <div class="notice">
          <Icon name="alert" size={17} />
          <span>
            Solo funciona en servidores con <span class="mono">online-mode=false</span>. En un
            servidor oficial (online-mode) te rechazará porque no hay sesión de Microsoft.
          </span>
        </div>
        {#if app.localSignInError}<p class="error">{app.localSignInError}</p>{/if}
        <div class="actions">
          <span class="spacer"></span>
          <button type="button" class="btn" onclick={() => app.closeLocalSignIn()} disabled={app.localSignInBusy}>
            Cancelar
          </button>
          <button type="submit" class="btn primary" disabled={app.localSignInBusy || username.trim().length < 3}>
            {app.localSignInBusy ? "Guardando…" : "Entrar"}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(0 0 0 / 0.55); display: grid; place-items: center; z-index: 30; }
  .modal { width: 460px; max-width: 92vw; background: rgb(var(--surface) / 1); }
  .title { display: flex; align-items: center; gap: 12px; }
  .title h1 { margin: 0; }
  .title p { margin: 3px 0 0; }
  .icon {
    width: 42px; height: 42px; flex: none; display: grid; place-items: center;
    border-radius: 12px; color: var(--accent);
    background: rgb(167 139 250 / 0.12); border: 1px solid var(--border-strong);
  }
  .notice {
    display: flex; gap: 10px; align-items: flex-start;
    padding: 11px 13px; border-radius: 10px; font-size: 12px; line-height: 1.45;
    color: var(--muted); background: rgb(250 204 21 / 0.07);
    border: 1px solid rgb(250 204 21 / 0.22);
  }
  .notice :global(svg) { flex: none; margin-top: 1px; color: rgb(250 204 21); }
  .actions { display: flex; gap: 10px; margin-top: 18px; }
  .spacer { flex: 1; }
</style>
