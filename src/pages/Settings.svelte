<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api, joinArgs, splitArgs } from "../lib/api";
  import { updates } from "../lib/updates.svelte";
  import Icon from "../components/Icon.svelte";
  import type { Settings, UpdateCheck } from "../lib/types";

  // Copia editable; se persiste solo al pulsar «Guardar».
  let draft = $state<Settings>(structuredClone($state.snapshot(app.settings!)));
  let jvm = $state(joinArgs(draft.minecraft.jvm_args));
  let dataDir = $state(draft.data_dir ?? "");
  let clientId = $state(draft.account.microsoft_client_id ?? "");
  let manifestUrl = $state(draft.updates.manifest_url ?? "");
  let sharingUrl = $state(draft.sharing.manifest_base_url ?? "");
  let error = $state("");
  let saved = $state(false);

  let checking = $state(false);
  let checkResult = $state<UpdateCheck | null>(null);
  let checkError = $state("");

  async function checkUpdates() {
    checking = true;
    checkError = "";
    try {
      checkResult = await api.checkForUpdates();
    } catch (e) {
      checkError = e instanceof Error ? e.message : String(e);
    } finally {
      checking = false;
    }
  }

  async function openDownload(url: string) {
    try {
      await api.openHttpsUrl(url);
    } catch (e) {
      checkError = e instanceof Error ? e.message : String(e);
    }
  }

  async function save() {
    error = "";
    saved = false;
    try {
      draft.minecraft.jvm_args = splitArgs(jvm);
      draft.data_dir = dataDir.trim() === "" ? null : dataDir.trim();
      draft.account.microsoft_client_id = clientId.trim() === "" ? null : clientId.trim();
      draft.updates.manifest_url = manifestUrl.trim() === "" ? null : manifestUrl.trim();
      draft.sharing.manifest_base_url = sharingUrl.trim() === "" ? null : sharingUrl.trim();
      await app.saveSettings($state.snapshot(draft) as Settings);
      saved = true;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<h1>Configuración</h1>
<br />

<section class="card">
  <h2>General</h2>
  <label class="field">
    <span>Directorio de datos de Nebula (vacío = predeterminado)</span>
    <input bind:value={dataDir} placeholder={app.info?.data_dir} />
  </label>
  <p class="hint">
    En uso ahora: <span class="mono">{app.info?.data_dir}</span>. Un cambio se aplica al reiniciar
    Nebula y no mueve las instancias existentes.
  </p>
</section>
<br />

<section class="card">
  <h2>Minecraft (valores para instancias nuevas)</h2>
  <div class="row">
    <label class="field"><span>RAM mínima (MB)</span><input type="number" step="256" bind:value={draft.minecraft.min_ram_mb} /></label>
    <label class="field"><span>RAM máxima (MB)</span><input type="number" step="256" bind:value={draft.minecraft.max_ram_mb} /></label>
  </div>
  <div class="row">
    <label class="field"><span>Ancho</span><input type="number" bind:value={draft.minecraft.width} /></label>
    <label class="field"><span>Alto</span><input type="number" bind:value={draft.minecraft.height} /></label>
  </div>
  <label class="field check"><input type="checkbox" bind:checked={draft.minecraft.fullscreen} /><span>Pantalla completa</span></label>
  <label class="field"><span>Argumentos JVM</span><textarea rows="2" bind:value={jvm}></textarea></label>
  <h2 style="margin-top:8px">Java encontrado</h2>
  {#if app.java.length === 0}
    <p class="muted">{app.javaLoading ? "Buscando…" : "No se encontró ningún Java. En Arch/CachyOS: sudo pacman -S jre21-openjdk (o jre17-openjdk, jre8-openjdk)."}</p>
  {/if}
  {#each app.java as j (j.path)}
    <p><strong>Java {j.major}</strong> <span class="muted mono">{j.version} · {j.path}</span></p>
  {/each}
  <br />
  <button class="btn" onclick={() => app.detectJava()} disabled={app.javaLoading}>Detectar de nuevo</button>
  <p class="hint">Cada instancia elige su Java en sus ajustes; en automático se usa el que pide su versión de Minecraft y, si falta, Nebula descarga el JRE de Adoptium solo.</p>
</section>
<br />

<section class="card">
  <h2>Interfaz</h2>
  <label class="field">
    <span>Tema</span>
    <select bind:value={draft.ui.theme}>
      <option value="dark">Oscuro</option>
      <option value="black">Negro</option>
      <option value="purple">Púrpura</option>
    </select>
  </label>
  <label class="field check"><input type="checkbox" bind:checked={draft.ui.animations} /><span>Animaciones</span></label>
  <label class="field check"><input type="checkbox" bind:checked={draft.ui.transparency} /><span>Transparencia de paneles</span></label>
</section>
<br />

<section class="card">
  <h2>Cuenta</h2>
  {#if app.account}
    <p>
      Conectado como <strong>{app.account.username}</strong>
      {#if app.account.kind === "offline"}
        <span class="pill-soft">cuenta local</span>
      {:else if app.account.flow === "legacy"}
        <span class="pill-soft">Microsoft heredado</span>
      {:else}
        <span class="pill-soft">Microsoft</span>
      {/if}.
    </p>
    {#if app.account.kind === "offline"}
      <p class="hint">
        Sin Microsoft ni Azure. Funciona en servidores con
        <span class="mono">online-mode=false</span>; en servidores oficiales será rechazada.
      </p>
      <button class="btn" onclick={() => app.openLocalSignIn()}>
        <Icon name="user" size={16} /> Cambiar nombre local
      </button>
    {:else if app.account.flow === "legacy"}
      <p class="hint">
        Sesión iniciada con el flujo heredado del launcher oficial. Funciona sin Azure, pero no es
        una integración oficial de Mojang: Microsoft podría bloquearla. Si algún día falla, usa una
        app de Azure propia o la cuenta local.
      </p>
    {/if}
    <br /><br />
    <button class="btn" onclick={() => app.signOut()}>
      <Icon name="logout" size={16} /> Cerrar sesión
    </button>
    <br /><br />
  {:else}
    <p>Sin cuenta. Puedes jugar en modo demo o iniciar sesión con una cuenta local.</p>
    <br />
    <button class="btn primary" onclick={() => app.openLocalSignIn()}>
      <Icon name="user" size={16} /> Iniciar sesión local
    </button>
    <br /><br />
  {/if}
  <details class="advanced">
    <summary>Microsoft (avanzado, requiere Azure)</summary>
    <label class="field">
      <span>Client ID de Microsoft</span>
      <input bind:value={clientId} placeholder="00000000-0000-0000-0000-000000000000" />
    </label>
    <p class="hint">
      Es el GUID de tu propia aplicación registrada en portal.azure.com, aprobada para la API de
      Minecraft. Sin esto, usa la cuenta local o el modo demo. Pasos completos en el README.
    </p>
    <button class="btn" onclick={() => app.startSignIn()} disabled={app.signInBusy}>
      Iniciar sesión con Azure
    </button>
  </details>
  <details class="advanced">
    <summary>Microsoft heredado (no oficial, sin Azure)</summary>
    <p class="hint">
      Usa el identificador público del launcher oficial de Minecraft, así que no necesitas
      registrar ninguna app. Tampoco es una integración oficial: Microsoft podría cambiarla o
      bloquearla sin aviso. Si tienes tu propia app de Azure, usa la opción anterior.
    </p>
    <button
      class="btn"
      onclick={() => app.startLegacySignIn()}
      disabled={app.signInBusy}
    >
      {app.signInBusy && app.signInLegacy ? "Esperando a Microsoft…" : "Iniciar sesión con Microsoft"}
    </button>
  </details>
  {#if app.signInError}<p class="error">{app.signInError}</p>{/if}
</section>
<br />

<section class="card">
  <h2>Compartir instancias</h2>
  <label class="field">
    <span>URL base del servicio de manifiestos (opcional)</span>
    <input bind:value={sharingUrl} placeholder="https://ejemplo.com/manifests" />
  </label>
  <p class="hint">
    Un código como <span class="mono">NBL-K7P2X9</span> se busca en
    <span class="mono">{"<esta URL>"}/NBL-K7P2X9.json</span>. Nebula no aloja nada: necesitas tu
    propio sitio (o uno de confianza) con esos archivos JSON. Sin esto, «Importar por código» no
    tiene dónde buscar. Más detalles en el README.
  </p>
</section>
<br />

<section class="card">
  <h2>Actualizaciones</h2>
  <p>Versión instalada: <span class="mono">{app.info?.version}</span></p>
  <br />
  <div class="update-actions">
    <button class="btn primary" onclick={() => updates.check()} disabled={updates.checking || updates.installing}>
      {updates.checking ? "Buscando…" : "Buscar actualizaciones"}
    </button>
    {#if updates.status && !updates.status.available}
      <span class="hint">Ya tienes la última versión.</span>
    {/if}
  </div>
  {#if updates.error}<p class="error">{updates.error}</p>{/if}
  <p class="hint">
    Nebula comprueba actualizaciones firmadas al iniciar y avisa cuando hay una versión nueva.
    Si aceptas, se descarga e instala sola y la aplicación se reinicia.
  </p>

  <details class="advanced">
    <summary>Manifiesto manual (avanzado)</summary>
    <label class="field">
      <span>URL del manifiesto de actualización (opcional)</span>
      <input bind:value={manifestUrl} placeholder="https://ejemplo.com/nebula/latest.json" />
    </label>
    <p class="hint">
      Fuente alternativa para comprobar a mano un <span class="mono">latest.json</span> propio
      (<span class="mono">{"{"}"version","url","notes{"}"}</span>) y abrir su descarga en el navegador.
    </p>
    <br />
    <button class="btn" onclick={checkUpdates} disabled={checking}>
      {checking ? "Buscando…" : "Comprobar manifiesto manual"}
    </button>
    {#if checkError}<p class="error">{checkError}</p>{/if}
    {#if checkResult}
      {#if checkResult.available}
        <p class="hint">
          Hay una versión nueva: <strong>v{checkResult.available.version}</strong>
          {#if checkResult.available.notes}— {checkResult.available.notes}{/if}
        </p>
        <button class="btn" onclick={() => openDownload(checkResult!.available!.url)}>Abrir descarga</button>
      {:else}
        <p class="hint">Ya tienes la última versión.</p>
      {/if}
    {/if}
  </details>
</section>
<br />

{#if error}<p class="error">{error}</p>{/if}
<button class="btn primary" onclick={save}>Guardar</button>
{#if saved}<span class="hint">&nbsp; Guardado.</span>{/if}

<style>
  .update-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .advanced {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }
  .advanced summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }
  .advanced[open] summary {
    margin-bottom: 14px;
  }
  .pill-soft {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--accent);
    background: rgb(167 139 250 / 0.12);
    border: 1px solid var(--border-strong);
  }
</style>
