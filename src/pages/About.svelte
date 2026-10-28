<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { api } from "../lib/api";
  import Icon from "../components/Icon.svelte";

  async function openRepository() {
    try {
      await api.openHttpsUrl("https://github.com/DevAldx/nebula-launcher");
    } catch {
      // Si el navegador no se puede abrir, la página sigue siendo informativa.
    }
  }
</script>

<h1>Acerca de Nebula Launcher</h1>
<p class="intro muted">
  Un launcher independiente para Minecraft: Java Edition, hecho para gestionar instancias,
  mods y cuentas sin depender del launcher oficial.
</p>

<section class="card hero">
  <img class="logo" src={app.launcherIcon ?? "/nebula-icon.png"} alt="" />
  <div class="hero-copy">
    <h2 class="product">Nebula Launcher</h2>
    <p class="muted">Versión {app.info?.version ?? "—"}</p>
    <p class="hint">
      Launcher de código abierto para Linux y Windows. No incluye Minecraft: usa los archivos
      oficiales que descarga desde los servidores de Mojang con tu propia cuenta.
    </p>
  </div>
  <button class="btn" onclick={openRepository}>
    <Icon name="info" size={16} /> Repositorio
  </button>
</section>

<section class="card">
  <h2>Copyright y marcas</h2>
  <div class="legal-row">
    <strong>Nebula Launcher</strong>
    <p>
      © 2026 Nebula Launcher. El código fuente se distribuye bajo la licencia MIT. El nombre,
      el icono y la identidad visual de Nebula pertenecen a sus autores.
    </p>
  </div>
  <div class="legal-row">
    <strong>Minecraft</strong>
    <p>
      Minecraft, el logotipo de Minecraft y los nombres relacionados son marcas registradas de
      Mojang Studios. © Mojang Studios. Todos los derechos reservados.
    </p>
  </div>
  <div class="legal-row">
    <strong>Microsoft</strong>
    <p>
      Microsoft, Xbox, Microsoft Account y los logotipos relacionados son marcas de Microsoft
      Corporation. El inicio de sesión con Microsoft se rige por los términos y la privacidad de
      Microsoft.
    </p>
  </div>
</section>

<section class="card">
  <h2>Aviso de no afiliación</h2>
  <p>
    Nebula Launcher es un proyecto independiente. No está afiliado, asociado, autorizado,
    respaldado ni patrocinado por Mojang Studios, Microsoft Corporation ni sus filiales.
  </p>
  <p>
    Nebula no incluye ni distribuye Minecraft. Descarga los archivos oficiales desde los
    servidores de Mojang usando tu propia cuenta. Las texturas, sonidos, fuentes y demás
    recursos del juego pertenecen a Mojang Studios y se usan únicamente como parte de la
    instalación y ejecución del juego.
  </p>
  <p class="hint">
    Los demás nombres y marcas mencionados pertenecen a sus respectivos propietarios. Si crees
    que algún contenido vulnera tus derechos, abre una incidencia en el repositorio.
  </p>
</section>

<section class="card">
  <h2>Créditos</h2>
  <p class="hint">
    Nebula usa las APIs públicas de Mojang para versiones, assets y autenticación de Minecraft,
    Fabric para el soporte de mods y Adoptium para descargar Java cuando hace falta. Gracias a
    sus mantenedores por hacerlo posible.
  </p>
</section>

<style>
  .intro {
    max-width: 680px;
    margin-top: -8px;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
  }
  .logo {
    width: 64px;
    height: 64px;
    border-radius: 16px;
    object-fit: cover;
    border: 1px solid rgb(255 255 255 / 0.14);
    box-shadow: 0 24px 48px -24px var(--accent-glow);
  }
  .hero-copy {
    flex: 1;
    min-width: 220px;
  }
  .product {
    margin: 0 0 2px;
    font-size: 18px;
    font-weight: 650;
    text-transform: none;
    letter-spacing: -0.01em;
    color: var(--text);
  }
  .hero .btn {
    margin-left: auto;
  }
  .legal-row {
    display: grid;
    grid-template-columns: 130px 1fr;
    gap: 14px;
    padding: 12px 0;
    border-top: 1px solid var(--border);
  }
  .legal-row:first-of-type {
    border-top: none;
    padding-top: 0;
  }
  .legal-row:last-of-type {
    padding-bottom: 0;
  }
  .legal-row strong {
    color: var(--text);
    font-weight: 600;
  }
  .legal-row p {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.65;
  }
  section > p {
    color: var(--muted);
    font-size: 13px;
    line-height: 1.7;
  }
  section > p + p {
    margin-top: 10px;
  }
  @media (max-width: 560px) {
    .legal-row {
      grid-template-columns: 1fr;
      gap: 4px;
    }
  }
</style>
