<script lang="ts">
  import { untrack } from "svelte";
  import { api, joinArgs, splitArgs } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import type { Instance, JavaChoice, Loader, VersionSummary } from "../lib/types";
  import { onMount } from "svelte";

  // `instance` presente = editar; ausente = crear.
  let { instance = null, onclose }: { instance?: Instance | null; onclose: () => void } = $props();

  // El formulario parte de los valores iniciales; el modal se recrea en cada apertura.
  const init = untrack(() => instance);
  const d = app.settings!.minecraft;
  let name = $state(init?.name ?? "");
  let version = $state(init?.minecraft_version ?? "");
  let loader = $state<Loader>(init?.loader ?? "fabric");
  let minRam = $state(init?.memory.min_mb ?? d.min_ram_mb);
  let maxRam = $state(init?.memory.max_mb ?? d.max_ram_mb);
  let jvm = $state(joinArgs(init?.jvm_args ?? d.jvm_args));
  let javaPath = $state(init?.java.kind === "path" ? init.java.path : "");
  let versions = $state<VersionSummary[]>([]);
  let showSnapshots = $state(false);
  let versionsError = $state("");
  const shown = $derived(versions.filter((v) => showSnapshots || v.kind === "release"));
  let error = $state("");

  onMount(async () => {
    if (init) return;
    try {
      versions = await api.listVersions();
      version = versions.find((v) => v.kind === "release")?.id ?? "";
    } catch (e) {
      versionsError = e instanceof Error ? e.message : String(e);
    }
  });
  let busy = $state(false);
  let confirmDelete = $state(false);

  async function submit() {
    busy = true;
    error = "";
    try {
      const memory = { min_mb: Number(minRam), max_mb: Number(maxRam) };
      if (instance) {
        const java: JavaChoice = javaPath ? { kind: "path", path: javaPath } : { kind: "auto" };
        await api.updateInstance(instance.id, { name, memory, jvm_args: splitArgs(jvm), java });
      } else {
        const created = await api.createInstance({
          name, minecraft_version: version, loader, memory, jvm_args: splitArgs(jvm),
        });
        if (!app.settings?.selected_instance) await app.select(created.id);
      }
      await app.refreshInstances();
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!instance) return;
    busy = true;
    try {
      app.settings = await api.deleteInstance(instance.id);
      await app.refreshInstances();
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      busy = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="card modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h1>{instance ? `Ajustes de ${instance.id}` : "Crear instancia"}</h1>
    <br />
    <label class="field"><span>Nombre</span><input bind:value={name} maxlength="48" /></label>
    <div class="row">
      <label class="field">
        <span>Versión de Minecraft</span>
        {#if instance}
          <input value={version} disabled />
        {:else}
          <select bind:value={version}>
            {#each shown as v (v.id)}<option value={v.id}>{v.id}</option>{/each}
          </select>
        {/if}
      </label>
      <label class="field">
        <span>Loader</span>
        <select bind:value={loader} disabled={!!instance}>
          <option value="vanilla">Vanilla</option>
          <option value="fabric">Fabric</option>
        </select>
      </label>
    </div>
    {#if !instance}
      <label class="field check">
        <input type="checkbox" bind:checked={showSnapshots} /><span>Mostrar snapshots y versiones antiguas</span>
      </label>
      {#if versionsError}<p class="error">No se pudo cargar la lista de versiones: {versionsError}</p>{/if}
    {/if}
    <div class="row">
      <label class="field"><span>RAM mínima (MB)</span><input type="number" step="256" bind:value={minRam} /></label>
      <label class="field"><span>RAM máxima (MB)</span><input type="number" step="256" bind:value={maxRam} /></label>
    </div>
    <label class="field">
      <span>Argumentos JVM (opcional)</span>
      <textarea rows="2" bind:value={jvm} placeholder="-XX:+UseG1GC"></textarea>
    </label>
    {#if instance}
      <label class="field">
        <span>Java</span>
        <select bind:value={javaPath}>
          <option value="">Automático (según la versión de Minecraft)</option>
          {#each app.java as j (j.path)}<option value={j.path}>Java {j.major} · {j.path}</option>{/each}
        </select>
      </label>
    {:else}
      <p class="hint">
        Java: automático. Nebula instalará el JRE exacto que pida esta versión de Minecraft si no
        está en el sistema.
      </p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}

    <div class="actions">
      {#if instance}
        {#if confirmDelete}
          <button class="btn danger" onclick={remove} disabled={busy}>Confirmar: borrar todo</button>
        {:else}
          <button class="btn danger" onclick={() => (confirmDelete = true)}>Eliminar</button>
        {/if}
      {/if}
      <span class="spacer"></span>
      <button class="btn" onclick={onclose}>Cancelar</button>
      <button class="btn primary" onclick={submit} disabled={busy || (!instance && !version)}>
        {instance ? "Guardar" : "Crear"}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgb(0 0 0 / 0.55); display: grid; place-items: center; }
  .modal { width: 480px; max-width: 92vw; background: rgb(var(--surface) / 1); }
  .actions { display: flex; gap: 10px; margin-top: 18px; }
  .spacer { flex: 1; }
</style>
