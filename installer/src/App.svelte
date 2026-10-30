<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  interface SourceInfo { path: string; size_bytes: number }
  interface InstallResult { installed_path: string; desktop_file: string }

  type Phase = "looking" | "found" | "picking" | "installing" | "done" | "error";
  let phase = $state<Phase>("looking");
  let source = $state<SourceInfo | null>(null);
  let step = $state(""); // fase actual durante la instalación, para feedback real
  let result = $state<InstallResult | null>(null);
  let error = $state("");
  let fileInput = $state<HTMLInputElement | null>(null);

  function formatSize(bytes: number): string {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  onMount(async () => {
    try {
      source = await invoke<SourceInfo | null>("find_launcher_appimage");
      phase = source ? "found" : "picking";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      phase = "picking";
    }
  });

  async function installFromFound() {
    if (!source) return;
    phase = "installing";
    error = "";
    try {
      step = "Verificando la firma y copiando archivos…";
      result = await invoke<InstallResult>("install_from_path", { sourcePath: source.path });
      step = "Creando acceso en el menú de aplicaciones…";
      phase = "done";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      phase = "error";
    }
  }

  function readAsBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const r = String(reader.result ?? "");
        resolve(r.slice(r.indexOf(",") + 1));
      };
      reader.onerror = () => reject(reader.error ?? new Error("no se pudo leer el archivo"));
      reader.readAsDataURL(file);
    });
  }

  async function onFileChosen(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    const file = files.find((f) => /\.appimage$/i.test(f.name));
    if (!file) {
      error = "Selecciona el archivo .AppImage.";
      phase = "error";
      return;
    }
    const sig = files.find((f) => f.name === `${file.name}.sig`);
    if (!sig) {
      error = "Selecciona también el archivo .sig que acompaña al AppImage.";
      phase = "error";
      return;
    }

    phase = "installing";
    error = "";
    try {
      step = "Leyendo y verificando el archivo elegido…";
      const [data_base64, signature] = await Promise.all([readAsBase64(file), sig.text()]);
      result = await invoke<InstallResult>("install_from_bytes", {
        fileName: file.name,
        dataBase64: data_base64,
        signature,
      });
      step = "Creando acceso en el menú de aplicaciones…";
      phase = "done";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      phase = "error";
    }
  }

  async function openNebula() {
    if (!result) return;
    try {
      await invoke("open_installed", { path: result.installed_path });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<main>
  <div class="brand">NEBULA</div>
  <p class="subtitle muted">Minecraft Launcher</p>
  <br />

  {#if phase === "looking"}
    <p class="muted">Buscando Nebula Launcher…</p>
  {:else if phase === "found" && source}
    <p>Se encontró <span class="mono">{source.path.split("/").pop()}</span> ({formatSize(source.size_bytes)}).</p>
    <br />
    <button class="btn primary" onclick={installFromFound}>Instalar</button>
    <br /><br />
    <button class="btn ghost" onclick={() => (phase = "picking")}>Elegir otro archivo…</button>
  {:else if phase === "picking"}
    <p class="muted">No se encontró un Nebula-Launcher.AppImage firmado junto al instalador.</p>
    <p class="muted small">Selecciona el AppImage y su archivo <span class="mono">.sig</span> (puedes elegir ambos a la vez).</p>
    <br />
    <input
      bind:this={fileInput}
      type="file"
      accept=".AppImage,.appimage,.sig"
      multiple
      hidden
      onchange={onFileChosen}
    />
    <button class="btn primary" onclick={() => fileInput?.click()}>Elegir AppImage y firma…</button>
  {:else if phase === "installing"}
    <p>Instalando…</p>
    <div class="bar"><div class="fill"></div></div>
    <p class="muted step">{step}</p>
  {:else if phase === "done" && result}
    <p class="done">Instalación completada</p>
    <p class="muted mono small">{result.installed_path}</p>
    <br />
    <button class="btn primary" onclick={openNebula}>Abrir Nebula</button>
  {:else if phase === "error"}
    <p class="error">No se pudo instalar.</p>
    <p class="error small">{error}</p>
    <br />
    <button class="btn" onclick={() => (phase = source ? "found" : "picking")}>Reintentar</button>
  {/if}
</main>

<style>
  main {
    height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center;
    text-align: center; padding: 32px;
  }
  .brand { font-weight: 800; letter-spacing: 0.35em; font-size: 28px; color: var(--accent); }
  .subtitle { letter-spacing: 0.1em; font-size: 12px; text-transform: uppercase; }
  .bar {
    width: 260px; height: 8px; border-radius: 6px; background: rgb(255 255 255 / 0.08);
    overflow: hidden; margin: 14px 0 8px;
  }
  .fill {
    height: 100%; width: 40%; border-radius: 6px; background: var(--accent);
    animation: slide 1.1s ease-in-out infinite alternate;
  }
  @keyframes slide { from { margin-left: 0; } to { margin-left: 60%; } }
  .step { font-size: 12px; }
  .done { color: var(--accent); font-weight: 650; font-size: 18px; }
  .small { font-size: 11px; word-break: break-all; max-width: 380px; }
</style>
