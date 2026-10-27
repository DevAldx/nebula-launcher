<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "./lib/api";
  import { applyUi } from "./lib/app.svelte";
  import type { AuthStateEvent, SignInCodeEvent } from "./lib/types";

  let code = $state<SignInCodeEvent | null>(null);
  let error = $state("");
  let copied = $state(false);
  let loading = $state(true);
  let unlisten: UnlistenFn[] = [];
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyCode() {
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code.user_code);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 2500);
    } catch {
      // El usuario puede seleccionar el código a mano.
    }
  }

  async function openBrowser() {
    if (!code) return;
    error = "";
    // Copiar antes de abrir el navegador ahorra el paso manual.
    await copyCode();
    try {
      await api.openUrl(code.verification_uri);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function closeWindow() {
    try {
      await api.cancelSignIn();
    } catch {
      // La ventana se cierra igualmente.
    }
  }

  onMount(() => {
    let disposed = false;

    void (async () => {
      try {
        const [current] = await Promise.all([
          api.getSignInCode(),
          api.getSettings().then((settings) => applyUi(settings.ui)).catch(() => {}),
        ]);
        if (!disposed) code = current;
      } catch (e) {
        if (!disposed) error = e instanceof Error ? e.message : String(e);
      } finally {
        if (!disposed) loading = false;
      }

      if (disposed) return;
      const offCode = await listen<SignInCodeEvent>("sign-in-code", (event) => {
        code = event.payload;
        error = "";
        loading = false;
      });
      const offAuth = await listen<AuthStateEvent>("auth-state", (event) => {
        if (event.payload.state === "error") {
          error = event.payload.message ?? "No se pudo iniciar sesión.";
        }
      });
      if (disposed) {
        offCode();
        offAuth();
      } else {
        unlisten.push(offCode, offAuth);
      }
    })();

    return () => {
      disposed = true;
      unlisten.forEach((off) => off());
      unlisten = [];
      if (copyTimer) clearTimeout(copyTimer);
    };
  });
</script>

<div class="signin-window">
  <header class="brand">
    <img class="brand-mark" src="/nebula-icon.png" alt="" />
    <div>
      <p class="eyebrow">Nebula Launcher</p>
      <h1>Iniciar sesión</h1>
    </div>
  </header>

  {#if loading}
    <p class="muted">Preparando el inicio de sesión…</p>
  {:else if code}
    <p class="muted">
      Abre la página de Microsoft y escribe este código. No lo compartas con nadie.
    </p>
    {#if code.legacy}
      <p class="hint">
        Método heredado: usa el identificador público del launcher oficial y no necesita una app de Azure.
      </p>
    {/if}

    <button type="button" class="code mono" onclick={copyCode} title="Copiar código">
      {code.user_code}
    </button>
    {#if copied}<p class="hint success">Código copiado.</p>{/if}

    <p class="hint link mono">{code.verification_uri}</p>

    {#if error}
      <p class="error">{error}</p>
      <div class="actions">
        <button type="button" class="btn primary" onclick={closeWindow}>Cerrar</button>
      </div>
    {:else}
      <div class="actions">
        <button type="button" class="btn primary" onclick={openBrowser}>
          Abrir en el navegador
        </button>
        <button type="button" class="btn" onclick={closeWindow}>Cancelar</button>
      </div>
      <p class="status"><span class="dot"></span> Esperando a que completes el inicio de sesión…</p>
    {/if}
  {:else}
    {#if error}<p class="error">{error}</p>{/if}
    <p class="muted">No hay un inicio de sesión en curso.</p>
    <p class="hint">Cierra esta ventana y vuelve a intentarlo desde Nebula.</p>
    <div class="actions">
      <button type="button" class="btn primary" onclick={closeWindow}>Cerrar</button>
    </div>
  {/if}
</div>

<style>
  .signin-window {
    height: 100%;
    overflow-y: auto;
    padding: 26px 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    background:
      radial-gradient(circle at 15% 0%, rgb(139 92 246 / 0.18), transparent 55%),
      radial-gradient(circle at 100% 100%, rgb(56 189 248 / 0.12), transparent 55%),
      var(--background);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 4px;
  }
  .brand-mark {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    object-fit: cover;
    border: 1px solid rgb(255 255 255 / 0.14);
    box-shadow: 0 30px 60px -40px var(--accent-glow);
  }
  .eyebrow {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--muted);
  }
  h1 {
    font-size: 22px;
    line-height: 1.2;
  }
  .code {
    display: block;
    width: 100%;
    font-size: 30px;
    letter-spacing: 0.12em;
    font-weight: 650;
    color: var(--accent);
    background: rgb(0 0 0 / 0.3);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px;
    cursor: pointer;
    user-select: text;
    text-align: center;
    transition: border-color var(--ease), background var(--ease);
  }
  .code:hover {
    border-color: var(--border-strong);
    background: rgb(0 0 0 / 0.4);
  }
  .actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .actions .btn {
    flex: 1;
    justify-content: center;
  }
  .link {
    word-break: break-all;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 0 var(--accent-glow);
    animation: pulse 1.6s infinite;
  }
  .success {
    color: var(--success);
  }
  @keyframes pulse {
    0% { box-shadow: 0 0 0 0 var(--accent-glow); }
    70% { box-shadow: 0 0 0 8px rgb(167 139 250 / 0); }
    100% { box-shadow: 0 0 0 0 rgb(167 139 250 / 0); }
  }
</style>
