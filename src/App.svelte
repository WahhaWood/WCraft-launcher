<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { onMount } from 'svelte'
  import { isTauri } from './lib/api'
  import Icon from './lib/Icon.svelte'
  import Logo from './lib/Logo.svelte'
  import { app, init, type Page } from './lib/state.svelte'
  import Play from './pages/Play.svelte'
  import Settings from './pages/Settings.svelte'

  const win = isTauri ? getCurrentWindow() : null
  const drawer = $derived(app.page === 'settings' || app.page === 'log')

  /** The window fills the screen: the button between minimize and close shows "restore" then. */
  let maximized = $state(false)

  const toggle = (page: Page) => (app.page = app.page === page ? 'play' : page)

  // The night scene is generated once from a fixed seed, so it looks the same on every start.
  let seed = 7
  const rand = () => ((seed = (seed * 16807) % 2147483647) / 2147483647)

  /** A stepped hill line: broad waves rounded to whole blocks, with a little noise. */
  function ridge(base: number, amp: number, block: number, phase: number, period: number) {
    let d = `M-40 800`
    for (let x = -40; x < 1240; x += block) {
      const h = Math.sin(x / period + phase) * amp + Math.sin(x / (period / 2.7) + phase * 2) * amp * 0.35 + (rand() - 0.5) * block
      d += `V${base - Math.round(h / block) * block}H${x + block}`
    }
    return d + 'V800Z'
  }

  const hills = [
    { d: ridge(470, 70, 20, 1.2, 190), fill: '#162340' },
    { d: ridge(560, 50, 20, 3.1, 150), fill: '#101a31' },
    { d: ridge(650, 26, 20, 0.4, 120), fill: '#0b1224' },
  ]
  const stars = Array.from({ length: 70 }, () => ({
    x: Math.round(rand() * 1200),
    y: Math.round(rand() * 380),
    s: rand() > 0.85 ? 3 : 2,
    o: (0.25 + rand() * 0.6).toFixed(2),
  }))

  onMount(() => {
    const requested = new URLSearchParams(location.search).get('page') as Page | null
    if (requested) app.page = requested
    void init()
    const sync = async () => win && (maximized = await win.isMaximized())
    void sync()
    // Maximizing by a double click on the header or by the system comes here too.
    const unlisten = win?.onResized(sync)
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && (app.page = app.page === 'log' ? 'settings' : 'play')
    window.addEventListener('keydown', esc)
    return () => {
      window.removeEventListener('keydown', esc)
      void unlisten?.then((f) => f())
    }
  })
</script>

<div class="shell">
  <svg class="scene" viewBox="0 0 1200 760" preserveAspectRatio="xMidYMax slice" aria-hidden="true">
    <defs>
      <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#060a15" />
        <stop offset=".62" stop-color="#14224a" />
        <stop offset="1" stop-color="#1d2f5a" />
      </linearGradient>
      <radialGradient id="moonglow">
        <stop offset="0" stop-color="#f6efcf" stop-opacity=".22" />
        <stop offset="1" stop-color="#f6efcf" stop-opacity="0" />
      </radialGradient>
      <radialGradient id="warm" cx=".5" cy="1" r=".6">
        <stop offset="0" stop-color="#f4b63f" stop-opacity=".16" />
        <stop offset="1" stop-color="#f4b63f" stop-opacity="0" />
      </radialGradient>
    </defs>
    <rect width="1200" height="760" fill="url(#sky)" />
    {#each stars as st}
      <rect x={st.x} y={st.y} width={st.s} height={st.s} fill="#dfe6ff" opacity={st.o} />
    {/each}
    <!-- The scene is pinned to the bottom: in a wide window the moon rises under the header, behind a side panel. -->
    <g class="moon" class:away={app.page !== 'play'}>
      <circle cx="1068" cy="162" r="110" fill="url(#moonglow)" />
      <g transform="translate(1046 140)">
        <rect width="44" height="44" fill="#f1ead0" />
        <rect x="8" y="8" width="10" height="10" fill="#d9d0b0" />
        <rect x="26" y="24" width="10" height="8" fill="#d9d0b0" />
        <rect x="12" y="30" width="6" height="6" fill="#e3dbbd" />
      </g>
    </g>
    <rect x="560" y="420" width="640" height="340" fill="url(#warm)" />
    {#each hills as h}<path d={h.d} fill={h.fill} />{/each}
  </svg>

  <header data-tauri-drag-region>
    <div class="left">
      <div class="brand">
        <Logo />
        <span>WCraft</span>
      </div>
    </div>

    <div class="tools">
      <button class="tool" class:on={app.page === 'skins'} onclick={() => toggle('skins')}>
        <Icon name="shirt" /> Скины
      </button>
      <button class="tool" class:on={app.page === 'shaders'} onclick={() => toggle('shaders')}>
        <Icon name="sparkle" /> Шейдеры
      </button>
      <button class="tool" class:on={app.page === 'resources'} onclick={() => toggle('resources')}>
        <Icon name="folder" /> Ресурспаки
      </button>
      {#if app.settings.experimental_mods}
        <button class="tool" class:on={app.page === 'mods'} onclick={() => toggle('mods')}>
          <Icon name="cube" /> Моды
        </button>
      {/if}
      <button class="tool" class:on={drawer} onclick={() => toggle('settings')}>
        <Icon name="sliders" /> Настройки
      </button>
      {#if win}
        <span class="sep"></span>
        <button class="win" aria-label="Свернуть" onclick={() => win.minimize()}><Icon name="minus" size={14} /></button>
        <button class="win" aria-label={maximized ? 'Восстановить' : 'Развернуть'} title={maximized ? 'Восстановить' : 'Развернуть'} onclick={() => win.toggleMaximize()}>
          <Icon name={maximized ? 'restore' : 'maximize'} size={14} />
        </button>
        <button class="win close" aria-label="Закрыть" onclick={() => win.close()}><Icon name="x" size={14} /></button>
      {/if}
    </div>
  </header>

  <main>
    <!-- One 3D player at a time: the skins page has its own; the shaders page covers it. -->
    {#if app.page !== 'skins' && app.page !== 'shaders' && app.page !== 'resources' && app.page !== 'mods'}<Play />{/if}

    {#if app.page === 'skins'}
      <div class="page">
        {#await import('./pages/Skins.svelte') then page}
          <page.default />
        {/await}
      </div>
    {/if}

    {#if app.page === 'shaders'}
      <div class="page">
        {#await import('./pages/Shaders.svelte') then page}
          <page.default />
        {/await}
      </div>
    {/if}

    {#if app.page === 'resources'}
      <div class="page">
        {#await import('./pages/Shaders.svelte') then page}
          <page.default kind="resource" />
        {/await}
      </div>
    {/if}

    {#if app.page === 'mods'}
      <div class="page">
        {#await import('./pages/Mods.svelte') then page}
          <page.default />
        {/await}
      </div>
    {/if}

    {#if drawer}
      <button class="scrim" aria-label="Закрыть настройки" onclick={() => (app.page = 'play')}></button>
      <aside class="drawer">
        <Settings />
      </aside>
    {/if}
  </main>
</div>

<style>
  .shell {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .scene {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    shape-rendering: crispEdges;
  }
  .moon {
    transition: opacity 0.25s;
  }
  .moon.away {
    opacity: 0;
  }

  header {
    position: relative;
    z-index: 5;
    height: 52px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 10px 0 20px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font: 600 20px var(--pixel);
    letter-spacing: 0.02em;
    pointer-events: none;
  }
  .left {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .tool {
    height: 34px;
    padding: 0 12px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink-soft);
    font-weight: 500;
    transition: color 0.12s, background 0.12s;
  }
  .tool:hover {
    color: var(--ink);
    background: rgba(255, 255, 255, 0.07);
  }
  .tool.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .sep {
    width: 1px;
    height: 18px;
    margin: 0 6px;
    background: var(--stroke);
  }
  .win {
    width: 32px;
    height: 28px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .win:hover {
    background: var(--surface-hi);
    color: var(--ink);
  }
  .win.close:hover {
    background: #d6384a;
    color: #fff;
  }

  main {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .page {
    position: absolute;
    inset: 0;
    animation: rise 0.18s ease-out;
  }
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 3;
    background: rgba(4, 7, 15, 0.6);
    animation: fade 0.18s;
    cursor: default;
  }
  .drawer {
    position: absolute;
    z-index: 4;
    top: 0;
    right: 0;
    bottom: 0;
    width: 480px;
    background: var(--panel);
    border-left: 1px solid var(--stroke);
    border-top-left-radius: 14px;
    box-shadow: -24px 0 60px -20px rgba(0, 0, 0, 0.7);
    animation: slide 0.2s ease-out;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes slide {
    from {
      transform: translateX(40px);
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      transform: translateY(10px);
      opacity: 0;
    }
  }
</style>
