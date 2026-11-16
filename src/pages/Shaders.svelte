<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type Shader } from '../lib/api'
  import Icon from '../lib/Icon.svelte'
  import { app } from '../lib/state.svelte'

  /** Shader packs, or the resource packs (no catalog of Horizon's own, several can be on). */
  let { kind = 'shader' }: { kind?: 'shader' | 'resource' } = $props()
  const res = $derived(kind === 'resource')
  let picker: HTMLInputElement
  let list = $state<Shader[] | null>(null)
  /** Why the list could not be read: no game installed yet, the CLI failed. */
  let failed = $state('')
  let error = $state('')
  /** Whose action the error is about: a shader's id, '' for "without shaders" and added files. */
  let errorAt = $state<string | null>(null)
  /** The shader an action runs on; '' for "without shaders" and added files. */
  let busy = $state<string | null>(null)
  let tab = $state<'catalog' | 'mine' | 'modrinth'>('modrinth')
  const mrTab = $derived(tab === 'modrinth')
  let query = $state('')
  let selectedId = $state<string | null>(null)
  let shot = $state(0)
  /** The picture shown over the whole window. */
  let zoom = $state(false)
  const step = (by: number) => {
    const n = selected?.screenshots.length ?? 0
    if (n) shot = (shot + by + n) % n
  }
  function keys(e: KeyboardEvent) {
    if (!zoom) return
    if (e.key === 'Escape') zoom = false
    else if (e.key === 'ArrowLeft') step(-1)
    else if (e.key === 'ArrowRight') step(1)
  }
  let dragging = $state(false)

  const mine = $derived((list ?? []).filter((s) => s.file))
  const catalog = $derived((list ?? []).filter((s) => !s.local))
  /** Modrinth's search results; they are asked for as the player types, not filtered here. */
  let mr = $state<Shader[]>([])
  let mrBusy = $state(false)
  let mrMore = $state(false)
  let mrFailed = $state('')
  let mrTimer: ReturnType<typeof setTimeout> | undefined
  const shown = $derived(
    tab === 'modrinth' ? mr : (tab === 'catalog' ? catalog : mine).filter((s) => {
      const q = query.trim().toLowerCase()
      return !q || [s.name, s.summary, ...s.authors, ...s.categories].some((t) => t.toLowerCase().includes(q))
    }),
  )
  const active = $derived(list?.find((s) => s.enabled))
  // Until the player picks one, the shader that is on, else the most downloaded.
  /** Modrinth's screenshots at full size by project, asked for when a project is picked. */
  let full = $state<Record<string, string[]>>({})
  const asked = new Set<string>()
  const picked = $derived(
    tab === 'modrinth'
      ? (mr.find((s) => s.id === selectedId) ?? mr[0])
      : (list?.find((s) => s.id === selectedId) ?? (res ? mine[0] : (active ?? catalog[0]))),
  )
  // The thumbnails search gives are other pictures than the project's own gallery starts with, so the preview
  // waits for the gallery rather than showing one picture and then another.
  const pending = $derived(tab === 'modrinth' && !!picked && full[picked.id] === undefined)
  const selected = $derived(picked && tab === 'modrinth' ? { ...picked, screenshots: full[picked.id] ?? [] } : picked)
  $effect(() => {
    const id = tab === 'modrinth' ? picked?.id : undefined
    if (!id || asked.has(id)) return
    asked.add(id)
    const fallback = picked?.screenshots ?? []
    api
      .modrinthGallery(id)
      .then(async (urls) => {
        // The first one is in the cache before it replaces the spinner.
        if (urls[0]) await new Promise((done) => Object.assign(new Image(), { onload: done, onerror: done, src: urls[0] }))
        full[id] = urls
      })
      .catch(() => (full[id] = fallback))
  })
  const locked = $derived(app.playing || busy !== null)

  const LABELS: Record<string, string> = {
    realistic: 'реалистичный',
    'semi-realistic': 'полуреалистичный',
    fantasy: 'сказочный',
    atmosphere: 'атмосфера',
    shadows: 'тени',
    reflections: 'отражения',
    'colored-lighting': 'цветной свет',
    screenshot: 'для скриншотов',
    low: 'лёгкий',
    medium: 'средний',
    high: 'тяжёлый',
  }
  const compact = new Intl.NumberFormat('ru', { notation: 'compact', maximumFractionDigits: 1 })
  const mb = (n: number) => (n < 1e6 ? `${Math.max(1, Math.round(n / 1e3))} КБ` : `${(n / 1e6).toFixed(1)} МБ`)
  /** The installed entry for a pack of any tab: its file is what turns on and off. */
  const installed = (s: Shader) => (s.file ? list?.find((l) => l.file === s.file) : undefined)
  const origin = (s: Shader) => (s.modrinth ? 'С Modrinth' : 'Свой файл')
  const outdated = (s: Shader) => !s.local && !!s.installed_version && s.installed_version !== s.version

  async function load() {
    try {
      list = await (res ? api.resourcesList() : api.shadersList())
      failed = ''
    } catch (e) {
      failed = String(e)
    }
  }

  /** Runs one change, then reads the list again: the CLI is what knows the result. */
  async function act(id: string, run: () => Promise<void>) {
    if (locked) return
    busy = id
    error = ''
    try {
      await run()
    } catch (e) {
      error = String(e)
      errorAt = id
    }
    await load()
    busy = null
  }

  /** A page of Modrinth's shaders for the query; `more` adds the next page to the list. */
  async function mrLoad(more = false) {
    if (mrBusy) return
    mrBusy = true
    const asked = query.trim()
    try {
      const page = await api.modrinthSearch(kind, asked, more ? mr.length : 0)
      if (asked !== query.trim()) return
      mr = more ? [...mr, ...page.filter((p) => !mr.some((m) => m.id === p.id))] : page
      mrMore = page.length >= 20
      mrFailed = ''
    } catch (e) {
      mrFailed = String(e)
    } finally {
      mrBusy = false
    }
    // The player kept typing while this was loading.
    if (asked !== query.trim() && tab === 'modrinth') void mrLoad()
  }

  function mrSearchSoon() {
    clearTimeout(mrTimer)
    if (tab === 'modrinth') mrTimer = setTimeout(() => void mrLoad(), 400)
  }

  function openModrinth() {
    tab = 'modrinth'
    selectedId = null
    shot = 0
    if (!mr.length) void mrLoad()
  }

  async function mrInstall(s: Shader) {
    await act(s.id, () => api.modrinthInstall(kind, s.id))
    if (error && errorAt === s.id) return
    const file = list?.find((l) => l.modrinth === s.id)?.file
    mr = mr.map((m) => (m.id === s.id ? { ...m, file: file ?? m.file } : m))
  }

  const turnOff = (s: Shader) => (res ? api.resourcesSet(s.file ?? s.id, false) : api.shadersSet(null))
  const turnOn = (s: Shader) => (res ? api.resourcesSet(s.file ?? s.id, true) : api.shadersSet(s.file))

  /** The row's switch: a shader turned on replaces the one that was on; turning it off leaves none. */
  function toggle(s: Shader) {
    const pack = installed(s)
    if (pack) void act(pack.id, () => (pack.enabled ? turnOff(pack) : turnOn(pack)))
  }

  function select(s: Shader) {
    selectedId = s.id
    shot = 0
    error = ''
  }

  async function addFile(file: File | undefined) {
    if (!file) return
    tab = 'mine'
    await act('', () => (res ? api.resourcesAdd(file) : api.shadersAdd(file)))
  }

  function drop(e: DragEvent) {
    e.preventDefault()
    dragging = false
    void addFile(e.dataTransfer?.files[0])
  }

  onMount(() => {
    // Modrinth is the page's first tab, for shaders and resource packs alike.
    void mrLoad()
    void load()
  })
</script>

<svelte:window onkeydown={keys} />

  {#if zoom && selected?.screenshots.length}
    <div class="zoom" role="dialog" aria-label="Снимок на весь экран">
      <button class="zoom-bg" aria-label="Закрыть" onclick={() => (zoom = false)}></button>
      <img src={selected.screenshots[shot]} alt="" draggable="false" />
      <button class="zoom-x" aria-label="Закрыть" onclick={() => (zoom = false)}><Icon name="x" size={20} /></button>
      {#if selected.screenshots.length > 1}
        <button class="zoom-arrow prev" aria-label="Предыдущий снимок" onclick={() => step(-1)}><Icon name="chevron" size={24} /></button>
        <button class="zoom-arrow next" aria-label="Следующий снимок" onclick={() => step(1)}><Icon name="chevron" size={24} /></button>
        <span class="zoom-count">{shot + 1} / {selected.screenshots.length}</span>
      {/if}
    </div>
  {/if}

<div
  class="shaders"
  role="region"
  ondragover={(e) => {
    e.preventDefault()
    dragging = true
  }}
  ondragleave={(e) => !e.relatedTarget && (dragging = false)}
  ondrop={drop}
>
  <section class="stage">
    <button class="back" onclick={() => (app.page = 'play')}><span class="left"><Icon name="chevron" /></span> На главную</button>

    {#if selected}
      {@const s = selected}
      <div class="preview">
        <div class="shot">
          {#if s.screenshots.length}
            {#key s.screenshots[shot]}
              <img src={s.screenshots[shot]} alt="" draggable="false" />
            {/key}
            <button class="enlarge" aria-label="Открыть на весь экран" onclick={() => (zoom = true)}></button>
            {#if s.screenshots.length > 1}
              <button class="arrow prev" aria-label="Предыдущий снимок" onclick={() => (shot = (shot + s.screenshots.length - 1) % s.screenshots.length)}>
                <Icon name="chevron" size={18} />
              </button>
              <button class="arrow next" aria-label="Следующий снимок" onclick={() => (shot = (shot + 1) % s.screenshots.length)}>
                <Icon name="chevron" size={18} />
              </button>
              <div class="dots">
                {#each s.screenshots as _, i}
                  <button class:on={i === shot} aria-label="Снимок {i + 1}" onclick={() => (shot = i)}></button>
                {/each}
              </div>
            {/if}
          {:else}
            <div class="blank">
              {#if pending}
                <span class="spinner"></span>
              {:else if s.icon}<img class="big-icon" src={s.icon} alt="" />{:else}<Icon name="sparkle" size={56} />{/if}
              <span>{pending ? 'Загружаю снимки…' : s.local ? (s.modrinth ? 'Снимки — на странице Modrinth' : 'Свой файл: снимков нет') : 'Автор не приложил снимков'}</span>
            </div>
          {/if}
          {#if s.enabled}<span class="on-badge"><Icon name="check" size={13} /> Включён</span>{/if}
        </div>

        <div class="info">
          <div class="title">
            {#if s.icon}<img class="icon" src={s.icon} alt="" />{/if}
            <div class="names">
              <h2 title={s.name}>{s.name}</h2>
              <p class="meta">
                {#if s.local}
                  {s.file}
                {:else}
                  {#if tab === 'modrinth'}
                    {s.authors.join(', ')} · {s.license} · {compact.format(s.downloads)} скачиваний
                  {:else}
                    {s.authors.join(', ')} · {s.installed_version && outdated(s) ? `${s.installed_version} → ${s.version}` : s.version} · {mb(s.size)} · {compact.format(s.downloads)} скачиваний
                  {/if}
                {/if}
              </p>
            </div>
          </div>
          {#if s.categories.length}
            <div class="tags">
              {#each s.categories as c}<span>{LABELS[c] ?? c}</span>{/each}
            </div>
          {/if}
          <p class="about">{s.description || s.summary}</p>

          <div class="actions">
            {#if tab === 'modrinth'}
              {#if s.file}
                <button class="btn" onclick={() => ((tab = 'mine'), (selectedId = s.file))}><Icon name="check" /> Установлен, открыть в «Мои»</button>
              {:else}
                <button class="btn primary" disabled={locked} onclick={() => mrInstall(s)}>
                  {#if busy === s.id}<span class="spinner dark"></span> Скачиваю…{:else}<Icon name="download" /> Установить и включить{/if}
                </button>
              {/if}
              <button class="btn ghost" onclick={() => api.openLink(s.page)}><Icon name="link" /> Modrinth</button>
            {:else if !s.file}
              <button class="btn primary" disabled={locked} onclick={() => act(s.id, () => api.shadersInstall(s.id))}>
                {#if busy === s.id}<span class="spinner dark"></span> Скачиваю…{:else}<Icon name="download" /> Установить и включить{/if}
              </button>
            {:else if s.enabled}
              <button class="btn" disabled={locked} onclick={() => act(s.id, () => turnOff(s))}>
                {#if busy === s.id}<span class="spinner"></span>{/if} Выключить
              </button>
            {:else}
              <button class="btn primary" disabled={locked} onclick={() => act(s.id, () => turnOn(s))}>
                {#if busy === s.id}<span class="spinner dark"></span>{:else}<Icon name="check" />{/if} Включить
              </button>
            {/if}
            {#if s.file && outdated(s)}
              <button class="btn" disabled={locked} onclick={() => act(s.id, () => api.shadersUpdate(s.id))}><Icon name="download" /> Обновить</button>
            {/if}
            {#if s.file}
              <button class="btn ghost danger" disabled={locked} aria-label="Удалить" title="Удалить" onclick={() => act(s.id, () => (res ? api.resourcesRemove(s.id) : api.shadersRemove(s.id)))}>
                <Icon name="trash" />
              </button>
            {/if}
            {#if s.page && tab !== 'modrinth'}
              <button class="btn ghost" onclick={() => api.openLink(s.page)}><Icon name="link" /> {s.modrinth ? 'Modrinth' : 'Страница'}</button>
            {/if}
          </div>
          {#if error && errorAt === s.id}<p class="error">{error}</p>{/if}
        </div>
      </div>
    {:else if tab === 'modrinth'}
      <p class="empty">{mrFailed || (mrBusy ? 'Ищу на Modrinth…' : 'Ничего не нашлось.')}</p>
    {:else if failed}
      <p class="empty">{failed}</p>
    {:else if list}
      <p class="empty">{res ? 'Ресурспаков пока нет.' : 'Шейдеров пока нет.'}</p>
    {/if}
  </section>

  <aside class="panel">
    <div class="top">
      <h1>{res ? 'Ресурспаки' : 'Шейдеры'}</h1>
      <div class="segment">
        <button class:on={tab === 'modrinth'} onclick={openModrinth}>Modrinth</button>
        {#if !res}<button class:on={tab === 'catalog'} onclick={() => (tab = 'catalog')}>Каталог <small>{catalog.length}</small></button>{/if}
        <button class:on={tab === 'mine'} onclick={() => (tab = 'mine')}>Мои <small>{mine.length}</small></button>
      </div>
    </div>
    <label class="search">
      <Icon name="search" />
      <input placeholder={tab === 'modrinth' ? 'Поиск на Modrinth' : 'Поиск по названию, автору, стилю'} bind:value={query} oninput={mrSearchSoon} />
    </label>

    <div class="scroll">
      {#if failed && !mrTab}
        <p class="none">{failed}</p>
      {:else if !list && !mrTab}
        <p class="none loading"><span class="spinner"></span> Загружаю каталог Horizon…</p>
      {:else}
        {#if mrTab && mrFailed}<p class="none">{mrFailed}</p>{/if}
        {#if tab === 'mine'}
          <button class="drop" class:over={dragging} disabled={locked} onclick={() => picker.click()}>
            <span class="drop-icon">{#if busy === ''}<span class="spinner dark"></span>{:else}<Icon name="plus" size={20} />{/if}</span>
            <span>
              <b>{res ? 'Добавить свой ресурспак' : 'Добавить свой шейдер'}</b>
              <small>{res ? '.zip' : '.zip или .hshader'}, перетащи файл сюда или нажми</small>
            </span>
          </button>
          {#if error && errorAt === ''}<p class="error">{error}</p>{/if}
        {/if}

        {#each shown as s (s.id)}
          {@const pack = installed(s)}
          <div class="line">
          <button class="row" class:picked={selected?.id === s.id} onclick={() => select(s)}>
            <span class="pic">
              {#if s.icon}<img src={s.icon} alt="" draggable="false" />{:else}<Icon name="sparkle" size={20} />{/if}
            </span>
            <span class="text">
              <b>{s.name}</b>
              <small>{s.local ? origin(s) : s.summary}</small>
            </span>
            {#if s.file && outdated(s)}<span class="pill">Обновить</span>{/if}
          </button>
          {#if pack}
            <button
              class="switch"
              class:on={pack.enabled}
              role="switch"
              aria-checked={pack.enabled}
              aria-label={pack.enabled ? 'Выключить' : 'Включить'}
              title={pack.enabled ? 'Выключить' : 'Включить'}
              disabled={locked}
              onclick={() => toggle(s)}
            >
              {#if busy === pack.id}<span class="spinner"></span>{:else}<span class="knob"></span>{/if}
            </button>
          {/if}
          </div>
        {:else}
          {#if mrTab}
            {#if mrBusy}<p class="none loading"><span class="spinner"></span> Ищу на Modrinth…</p>{/if}
          {:else if tab === 'catalog' || query}
            <p class="none">{query ? 'Ничего не нашлось.' : 'Каталог пуст: Horizon не ответил.'}</p>
          {/if}
        {/each}
        {#if mrTab && mr.length && mrMore}
          <button class="drop" disabled={mrBusy} onclick={() => mrLoad(true)}>
            <span class="drop-icon">{#if mrBusy}<span class="spinner dark"></span>{:else}<Icon name="plus" size={20} />{/if}</span>
            <span><b>Показать ещё</b></span>
          </button>
        {/if}
      {/if}
    </div>

    <div class="note">
      {#if app.playing}
        Игра запущена: шейдер меняется, когда она закрыта.
      {:else}
        {res ? 'Включённые паки накладываются, новый — сверху. Применится при следующем запуске игры.' : 'Включается один шейдер. Он применится при следующем запуске игры.'}
        <button class="folder" onclick={() => (res ? api.resourcesOpenDir() : api.shadersOpenDir()).catch((e) => ((error = String(e)), (errorAt = '')))}><Icon name="folder" size={14} /> Папка</button>
      {/if}
    </div>
  </aside>

  <input bind:this={picker} type="file" accept={res ? '.zip' : '.zip,.hshader'} hidden onchange={() => (addFile(picker.files?.[0]), (picker.value = ''))} />
</div>

<style>
  .shaders {
    display: grid;
    grid-template-columns: 1fr 460px;
    height: 100%;
  }

  .stage {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 48px 28px 24px 24px;
    min-width: 0;
    min-height: 0;
  }
  .back {
    position: absolute;
    top: 6px;
    left: 20px;
    height: 32px;
    padding: 0 12px 0 6px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--ink-soft);
    font-weight: 500;
  }
  .back:hover {
    color: var(--ink);
    background: rgba(255, 255, 255, 0.07);
  }
  .left {
    display: flex;
    transform: rotate(90deg);
  }
  .empty {
    margin: auto;
    max-width: 280px;
    text-align: center;
    color: var(--muted);
  }

  .preview {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-height: 0;
    max-width: 760px;
    width: 100%;
    margin: 0 auto;
    animation: appear 0.2s;
  }
  .shot {
    position: relative;
    flex: none;
    aspect-ratio: 16 / 9;
    max-height: 46vh;
    border-radius: 12px;
    overflow: hidden;
    background: linear-gradient(180deg, #1b2a4f, #111a31);
    box-shadow: 0 20px 50px -20px rgba(0, 0, 0, 0.8), inset 0 0 0 1px var(--rule);
  }
  .shot > img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    animation: appear 0.25s;
  }
  .enlarge {
    position: absolute;
    inset: 0;
    cursor: zoom-in;
  }
  .arrow,
  .dots {
    z-index: 1;
  }
  .zoom {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: rgba(2, 4, 10, 0.94);
    animation: appear 0.15s;
  }
  .zoom-bg {
    position: absolute;
    inset: 0;
    cursor: zoom-out;
  }
  .zoom > img {
    position: relative;
    max-width: 96vw;
    max-height: 96vh;
    object-fit: contain;
    pointer-events: none;
  }
  .zoom-x,
  .zoom-arrow {
    position: absolute;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
  }
  .zoom-x:hover,
  .zoom-arrow:hover {
    background: rgba(255, 255, 255, 0.25);
  }
  .zoom-x {
    top: 18px;
    right: 18px;
  }
  .zoom-arrow {
    top: 50%;
    margin-top: -22px;
  }
  .zoom-arrow.prev {
    left: 18px;
    transform: rotate(90deg);
  }
  .zoom-arrow.next {
    right: 18px;
    transform: rotate(-90deg);
  }
  .zoom-count {
    position: absolute;
    bottom: 18px;
    padding: 4px 12px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
    font-size: 13px;
  }
  .blank {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--muted);
  }
  .big-icon {
    width: 96px;
    height: 96px;
    border-radius: 16px;
    image-rendering: pixelated;
  }
  .arrow {
    position: absolute;
    top: 50%;
    width: 36px;
    height: 36px;
    margin-top: -18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: rgba(4, 7, 15, 0.55);
    color: #fff;
    opacity: 0;
    transition: opacity 0.15s, background 0.12s;
  }
  .shot:hover .arrow {
    opacity: 1;
  }
  .arrow:hover {
    background: rgba(4, 7, 15, 0.8);
  }
  .arrow.prev {
    left: 10px;
    transform: rotate(90deg);
  }
  .arrow.next {
    right: 10px;
    transform: rotate(-90deg);
  }
  .dots {
    position: absolute;
    bottom: 10px;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
    gap: 6px;
  }
  .dots button {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.4);
  }
  .dots button.on {
    background: #fff;
  }
  .on-badge {
    position: absolute;
    top: 10px;
    left: 10px;
    height: 26px;
    padding: 0 10px 0 8px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 12px;
    font-weight: 600;
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .icon {
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: 10px;
    image-rendering: pixelated;
  }
  .names {
    min-width: 0;
  }
  h2 {
    font: 600 22px/1.2 var(--font);
    letter-spacing: -0.01em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--muted);
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tags span {
    height: 22px;
    padding: 0 9px;
    border-radius: 99px;
    display: inline-flex;
    align-items: center;
    background: var(--surface-hi);
    color: var(--ink-soft);
    font-size: 12px;
  }
  .about {
    color: var(--ink-soft);
    font-size: 13px;
    max-height: 4.5em;
    overflow-y: auto;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .actions .btn {
    height: 36px;
  }
  .spinner.dark {
    border-color: rgba(0, 0, 0, 0.2);
    border-top-color: var(--accent-ink);
  }

  /* The list panel */
  .panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin: 0 20px 20px 0;
    border-radius: 14px;
    background: rgba(9, 13, 28, 0.94);
    border: 1px solid var(--stroke);
    box-shadow: 0 20px 50px -20px rgba(0, 0, 0, 0.8);
    backdrop-filter: blur(14px);
    overflow: hidden;
  }
  .top {
    padding: 20px 20px 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h1 {
    font: 600 22px var(--font);
    letter-spacing: -0.01em;
  }
  .top .segment button {
    padding: 0 12px;
  }
  .segment small {
    margin-left: 4px;
    opacity: 0.6;
    font-size: 12px;
  }
  .search {
    margin: 0 20px 10px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--radius);
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid var(--stroke);
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }
  .search:focus-within {
    border-color: var(--accent-soft);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    user-select: text;
    -webkit-user-select: text;
  }
  .search input::placeholder {
    color: var(--muted);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 12px 12px;
  }

  .row {
    width: 100%;
    padding: 8px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
    transition: background 0.12s, box-shadow 0.12s;
  }
  .row:hover:not(:disabled) {
    background: var(--surface-hi);
  }
  .row.picked {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .line {
    position: relative;
    display: flex;
    align-items: center;
  }
  /* Room on the right of a row for its switch. */
  .line:has(.switch) .row {
    padding-right: 58px;
  }
  .switch {
    position: absolute;
    right: 10px;
    width: 38px;
    height: 22px;
    border-radius: 11px;
    background: var(--surface-hi);
    box-shadow: inset 0 0 0 1px var(--stroke);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s;
  }
  .switch.on {
    background: var(--accent);
    box-shadow: none;
  }
  .switch .knob {
    position: absolute;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--ink-soft);
    transition: transform 0.15s, background 0.15s;
  }
  .switch.on .knob {
    transform: translateX(16px);
    background: var(--accent-ink);
  }
  .switch .spinner {
    width: 12px;
    height: 12px;
  }
  .pic {
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: 9px;
    overflow: hidden;
    display: grid;
    place-items: center;
    background: linear-gradient(180deg, #1b2a4f, #111a31);
    color: var(--muted);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .pic img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    image-rendering: pixelated;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .text b {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text small {
    color: var(--muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    flex: none;
    height: 22px;
    padding: 0 8px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    background: var(--surface-hi);
    color: var(--ink-soft);
    font-size: 12px;
    font-weight: 500;
  }

  .drop {
    width: 100%;
    margin: 6px 0 10px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1.5px dashed var(--stroke);
    display: flex;
    align-items: center;
    gap: 14px;
    text-align: left;
    transition: border-color 0.12s, background 0.12s;
  }
  .drop:hover:not(:disabled),
  .drop.over {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .drop-icon {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 8px;
    display: grid;
    place-items: center;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .drop b {
    display: block;
    font-weight: 600;
  }
  .drop small {
    color: var(--muted);
    font-size: 12px;
  }

  .error {
    color: var(--danger);
    font-size: 13px;
  }
  .none {
    color: var(--muted);
    padding: 8px;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .note {
    padding: 10px 12px 10px 20px;
    border-top: 1px solid var(--rule);
    color: var(--muted);
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .folder {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-soft);
  }
  .folder:hover {
    color: var(--ink);
    background: var(--surface-hi);
  }
  @keyframes appear {
    from {
      opacity: 0;
    }
  }
</style>
