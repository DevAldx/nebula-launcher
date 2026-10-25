<script lang="ts">
  // La textura de Minecraft es un atlas de 64×64. La cara está en (8,8) y el
  // sombrero en (40,8), ambos de 8×8; se recortan con CSS y `pixelated` para
  // que se vea la cara nítida en vez de la textura completa.
  let {
    url = null,
    username,
    size = 38,
  }: { url?: string | null; username: string; size?: number } = $props();

  let failed = $state(false);

  $effect(() => {
    const candidate = url;
    failed = false;
    if (!candidate) return;
    let alive = true;
    const probe = new Image();
    probe.onerror = () => {
      if (alive && candidate === url) failed = true;
    };
    probe.src = candidate;
    return () => {
      alive = false;
    };
  });

  const initial = $derived(username.trim().slice(0, 1).toUpperCase() || "?");
  const faceStyle = $derived(
    `width:${size}px;height:${size}px;--skin:url("${(url ?? "").replaceAll('"', "%22")}")`,
  );
</script>

{#if url && !failed}
  <span
    class="skin-face"
    style={faceStyle}
    role="img"
    aria-label={`Cara de la skin de ${username}`}
    title={`Skin de ${username}`}
  >
    <span class="hat"></span>
  </span>
{:else}
  <span
    class="skin-face fallback"
    style={`width:${size}px;height:${size}px;font-size:${Math.round(size * 0.42)}px`}
    role="img"
    aria-label={`Inicial de ${username}`}
  >
    {initial}
  </span>
{/if}

<style>
  .skin-face {
    position: relative;
    display: inline-grid;
    place-items: center;
    flex: none;
    overflow: hidden;
    border-radius: 26%;
    border: 1px solid rgb(255 255 255 / 0.14);
    background-color: rgb(167 139 250 / 0.12);
    image-rendering: pixelated;
  }
  .skin-face:not(.fallback) {
    background-image: var(--skin);
    background-size: 800% 800%;
    background-repeat: no-repeat;
    /* (8/56) y (40/56) con background-size al 800 %. */
    background-position: -14.2857% -14.2857%;
  }
  .hat {
    position: absolute;
    inset: 0;
    background-image: var(--skin);
    background-size: 800% 800%;
    background-repeat: no-repeat;
    background-position: -71.4286% -14.2857%;
    image-rendering: pixelated;
  }
  .fallback {
    font-weight: 750;
    color: var(--accent);
    background: rgb(167 139 250 / 0.14);
    border-color: var(--border-strong);
  }
</style>
