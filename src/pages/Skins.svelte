<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type CatalogPage, type CatalogSkin, type Skin } from '../lib/api'
  import Character from '../lib/Character.svelte'
  import Icon from '../lib/Icon.svelte'
  import { inspect, thumbnail } from '../lib/skin'
  import { activeSkin, app, loadSkins } from '../lib/state.svelte'

  let picker: HTMLInputElement
  let dragging = $state(false)
  let error = $state('')

  const lib = $derived(app.skins)
  const current = $derived(activeSkin())
  const isSaved = $derived(!!current && !current.id.startsWith('default:'))
  let winH = $state(740)
  const charH = $derived(Math.max(340, Math.min(540, winH - 210)))
  let tab = $state<'catalog' | 'saved'>('catalog')
  const shown = $derived(lib?.saved ?? [])

  async function select(skin: Skin) {
    trying = null
    if (!lib || lib.active === skin.id) return
    lib.active = skin.id
    await api.skinsSelect(skin.id)
  }

  // Online catalog (millida.net): an empty query lists the most worn skins first.
  let query = $state('')
  let items = $state<CatalogSkin[]>([])
  let page = $state(0)
  let pages = $state(1)
  let loading = $state(false)
  let installing = $state('')
  let catalogError = $state('')
  let timer: ReturnType<typeof setTimeout> | undefined
  let seq = 0
  /** Pages already fetched, by `query\npage`: going back to a search shows it at once. */
  const cache = new Map<string, CatalogPage>()
  /** Browsing by tag and order: titles are generated descriptions, guessing them is hard. */
  let tag = $state('')
  let sort = $state<'popular' | 'new' | 'random'>('popular')
  // The catalog's tags that give a few hundred skins or more and are about a theme, not a colour (counts of 2026-11-13).
  const TAGS = [
    ['dlya-devushek', 'для девушек'],
    ['dlya-malchikov', 'для парней'],
    ['anime', 'аниме'],
    ['milye', 'милые'],
    ['krutye', 'крутые'],
    ['smeshnye', 'смешные'],
    ['strashnye', 'страшные'],
    ['fentezi', 'фэнтези'],
    ['voiny', 'воины'],
    ['soldaty', 'военные'],
    ['pvp', 'PvP'],
    ['moby', 'мобы'],
    ['koshki', 'кошки'],
    ['s-zhivotnymi', 'с животными'],
    ['v-hudi', 'в худи'],
    ['v-kostyume', 'в костюме'],
    ['v-maske', 'в маске'],
    ['v-plate', 'в платье'],
  ]
  const SORTS = { popular: 'Популярные', new: 'Новые', random: 'Случайные' } as const
  const keyOf = (n: number) => `${query.trim()}\n${tag}\n${sort}\n${n}`

  /** The first page of `query` (`next` = false) or the page after the shown ones. */
  async function load(next: boolean) {
    if (next && (loading || page >= pages)) return
    const mine = ++seq
    const q = query.trim()
    const want = next ? page + 1 : 1
    const key = keyOf(want)
    catalogError = ''
    // A random order is new every time it is asked for.
    let res = sort === 'random' && !next ? undefined : cache.get(key)
    if (!res) {
      loading = true
      try {
        res = await api.skinsCatalog(q, want, tag, sort)
        cache.set(key, res)
      } catch (e) {
        if (mine === seq) {
          catalogError = String(e)
          loading = false
        }
        return
      }
      if (mine !== seq) return
      loading = false
    }
    const seen = new Set(next ? items.map((i) => i.id) : [])
    items = next ? [...items, ...res.items.filter((i) => !seen.has(i.id))] : res.items
    page = want
    pages = res.pages
  }

  function typing() {
    clearTimeout(timer)
    // A cached search needs no pause.
    if (cache.has(keyOf(1))) return void load(false)
    timer = setTimeout(() => load(false), 300)
  }

  /** Loads the next page when the end of the list scrolls into view. */
  function sentinel(node: HTMLElement) {
    const io = new IntersectionObserver((e) => e[0].isIntersecting && void load(true), { rootMargin: '400px' })
    io.observe(node)
    return { destroy: () => io.disconnect() }
  }

  /** A catalog skin tried on: shown on the stage, saved only when the player adds it. */
  let trying = $state<{ skin: CatalogSkin; view: Skin } | null>(null)
  let fetching = $state('')
  const viewing = $derived(trying?.view ?? current)

  async function tryOn(skin: CatalogSkin) {
    if (trying?.skin.id === skin.id) return
    fetching = skin.id
    catalogError = ''
    try {
      const url = await api.skinsCatalogPng(skin.id)
      if (fetching !== skin.id) return
      trying = { skin, view: { id: `catalog:${skin.id}`, name: skin.title, model: skin.model, url } }
    } catch (e) {
      catalogError = String(e)
    } finally {
      if (fetching === skin.id) fetching = ''
    }
  }

  async function install() {
    if (!trying || installing) return
    const skin = trying.skin
    installing = skin.id
    catalogError = ''
    try {
      await api.skinsCatalogInstall(skin.id, skin.title, skin.model)
      await loadSkins()
      trying = null
    } catch (e) {
      catalogError = String(e)
    } finally {
      installing = ''
    }
  }

  function pickTag(slug: string) {
    tag = tag === slug ? '' : slug
    void load(false)
  }

  function pickSort(s: typeof sort) {
    sort = s
    void load(false)
  }

  /** A preview the renderer did not give: ask again twice, a little later each time. */
  function retry(img: HTMLImageElement, url: string) {
    const n = Number(img.dataset.retry ?? 0)
    if (n >= 2) return
    img.dataset.retry = String(n + 1)
    setTimeout(() => (img.src = `${url}&retry=${n + 1}`), 1500 * (n + 1))
  }

  onMount(() => void load(false))

  async function add(file: File | undefined) {
    if (!file) return
    error = ''
    tab = 'saved'
    try {
      const { model, png } = await inspect(file)
      await api.skinsAdd(file.name.replace(/\.png$/i, ''), model, png)
      await loadSkins()
    } catch (e) {
      error = String(e)
    }
  }

  async function setModel(model: 'classic' | 'slim') {
    if (!current || current.model === model) return
    await api.skinsSetModel(current.id, model)
    await loadSkins()
  }

  /** Removes the worn skin and puts on its neighbour in «Мои», or a default one when none is left. */
  async function remove() {
    if (!current || !lib) return
    const saved = lib.saved
    const at = saved.findIndex((s) => s.id === current.id)
    const next = saved[at + 1] ?? saved[at - 1] ?? lib.defaults[0]
    error = ''
    try {
      await api.skinsRemove(current.id)
      if (next) await api.skinsSelect(next.id)
    } catch (e) {
      error = String(e)
    }
    await loadSkins()
  }

  function drop(e: DragEvent) {
    e.preventDefault()
    dragging = false
    void add(e.dataTransfer?.files[0])
  }
</script>

<svelte:window bind:innerHeight={winH} />

<div
  class="skins"
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

    <div class="figure">
      {#if viewing}
        <Character skin={viewing} zoom={1} width={Math.round(charH * 0.7)} height={charH} />
      {:else}
        <p class="empty">Выбери скин справа или добавь свой.</p>
      {/if}
    </div>

    {#if trying}
      <div class="caption">
        <p class="name" title={trying.view.name}>{trying.view.name}</p>
        <p class="hint"><Icon name="rotate" size={14} /> Примерка из каталога · {trying.skin.model === 'slim' ? 'тонкие руки' : 'обычные руки'}</p>
      </div>
      <div class="tools">
        <button class="btn primary" disabled={!!installing} onclick={install}>
          {#if installing}<span class="spinner dark"></span>{:else}<Icon name="plus" />{/if} Добавить в «Мои» и надеть
        </button>
        <button class="btn ghost" onclick={() => (trying = null)}>Отмена</button>
      </div>
    {:else if current}
      <div class="caption">
        <p class="name" title={current.name}>{current.name}</p>
        <p class="hint"><Icon name="rotate" size={14} /> Потяни, чтобы повернуть</p>
      </div>
      {#if isSaved}
        <div class="tools">
          <div class="segment">
            <button class:on={current.model === 'classic'} onclick={() => setModel('classic')}>Обычные руки</button>
            <button class:on={current.model === 'slim'} onclick={() => setModel('slim')}>Тонкие</button>
          </div>
          <button class="btn ghost danger" aria-label="Удалить скин" title="Удалить скин" onclick={remove}><Icon name="trash" /></button>
        </div>
      {/if}
    {/if}
  </section>

  <aside class="wardrobe">
    <div class="top">
      <h1>Гардероб</h1>
      <div class="segment">
        <button class:on={tab === 'catalog'} onclick={() => (tab = 'catalog')}>Каталог</button>
        <button class:on={tab === 'saved'} onclick={() => ((tab = 'saved'), (trying = null))}>Мои <small>{lib?.saved.length ?? 0}</small></button>
      </div>
    </div>

    <div class="scroll">
      {#if tab === 'catalog'}
        <label class="search">
          <Icon name="search" />
          <input placeholder="Поиск: рыцарь, кот в шляпе…" spellcheck="false" bind:value={query} oninput={typing} />
        </label>
        <div class="sorts">
          {#each Object.entries(SORTS) as [key, label]}
            <button class:on={sort === key} onclick={() => pickSort(key as typeof sort)}>{label}</button>
          {/each}
        </div>
        <div class="tags">
          {#each TAGS as [slug, label] (slug)}
            <button class:on={tag === slug} onclick={() => pickTag(slug)}>{label}</button>
          {/each}
        </div>
        {#if catalogError}<p class="error">{catalogError}</p>{/if}
        {#if items.length}
          <div class="grid">
            {#each items as skin (skin.id)}
              {@const have = lib?.saved.some((m) => m.name === skin.title)}
              <button class="card" class:on={trying?.skin.id === skin.id} title={have ? `${skin.title} — уже в «Мои»` : `${skin.title} — примерить`} onclick={() => tryOn(skin)}>
                <!-- No loading="lazy": WebKitGTK never loads lazy images below the fold of a scrolled box. -->
                <img src={skin.preview} alt="" draggable="false" decoding="async" onerror={(e) => retry(e.currentTarget as HTMLImageElement, skin.preview)} />
                <span class="label">{skin.title}</span>
                {#if fetching === skin.id}<span class="badge"><span class="spinner"></span></span>{:else if have}<span class="badge"><Icon name="check" size={13} /></span>{/if}
              </button>
            {/each}
          </div>
          {#if page < pages}
            <div class="more" use:sentinel>{#if loading}<span class="spinner"></span>{/if}</div>
          {/if}
        {:else if loading}
          <p class="none loading"><span class="spinner"></span> Загружаю каталог…</p>
        {:else if !catalogError}
          <p class="none">Ничего не найдено.</p>
        {/if}
      {:else}
        <button class="drop" class:over={dragging} onclick={() => picker.click()}>
          <span class="drop-icon"><Icon name="plus" size={20} /></span>
          <span>
            <b>Добавить свой скин</b>
            <small>PNG 64×64, перетащи файл сюда или нажми</small>
          </span>
        </button>
        {#if error}<p class="error">{error}</p>{/if}
        {#if shown.length}
          <div class="grid">
            {#each shown as skin (skin.id)}
              <button class="card" class:on={lib?.active === skin.id} title={skin.name} onclick={() => select(skin)}>
                {#await thumbnail(skin) then src}
                  <img {src} alt="" draggable="false" />
                {/await}
                <span class="label">{skin.name}</span>
                {#if lib?.active === skin.id}<span class="badge"><Icon name="check" size={13} /></span>{/if}
              </button>
            {/each}
          </div>
        {:else}
          <p class="none">Своих скинов пока нет. Найди в «Каталоге» или добавь файл.</p>
        {/if}
      {/if}
    </div>

    <p class="note">Скин применится при следующем запуске игры.</p>
  </aside>

  <input bind:this={picker} type="file" accept="image/png" hidden onchange={() => (add(picker.files?.[0]), (picker.value = ''))} />
</div>

<style>
  .search {
    margin-bottom: 12px;
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
  .sorts {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
  }
  .sorts button,
  .tags button {
    height: 28px;
    padding: 0 11px;
    border-radius: 14px;
    font-size: 12.5px;
    color: var(--ink-soft);
    background: var(--surface-hi);
    box-shadow: inset 0 0 0 1px var(--stroke);
  }
  .sorts button:hover,
  .tags button:hover {
    color: var(--ink);
  }
  .sorts button.on,
  .tags button.on {
    background: var(--accent);
    color: var(--accent-ink);
    box-shadow: none;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
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
  .more {
    display: flex;
    justify-content: center;
    min-height: 40px;
    padding-top: 12px;
  }

  .skins {
    display: grid;
    grid-template-columns: 1fr 460px;
    height: 100%;
  }

  /* The player on the hill */
  .stage {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 0 24px 28px;
    min-width: 0;
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
  .figure {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .empty {
    margin: auto;
    max-width: 220px;
    text-align: center;
    color: var(--muted);
  }
  .caption {
    text-align: center;
  }
  .name {
    font: 600 18px var(--font);
    max-width: 360px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }
  .tools .segment {
    background: rgba(6, 10, 22, 0.6);
  }
  .tools .btn {
    padding: 0 9px;
  }
  .tools .btn.primary {
    padding: 0 16px;
  }

  /* Wardrobe panel */
  .wardrobe {
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
    padding: 20px 20px 14px;
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
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 20px 16px;
  }

  .drop {
    width: 100%;
    margin-bottom: 14px;
    padding: 12px 14px;
    border-radius: 10px;
    border: 1.5px dashed var(--stroke);
    display: flex;
    align-items: center;
    gap: 14px;
    text-align: left;
    transition: border-color 0.12s, background 0.12s;
  }
  .drop:hover,
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

  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .card {
    position: relative;
    aspect-ratio: 15 / 19;
    border-radius: 10px;
    background: linear-gradient(180deg, #1b2a4f, #111a31);
    overflow: hidden;
    box-shadow: inset 0 0 0 1px var(--rule);
    transition: transform 0.12s, box-shadow 0.12s;
  }
  .card:hover {
    transform: translateY(-2px);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.28);
  }
  .card.on {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .card img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top;
    animation: appear 0.2s;
  }
  @keyframes appear {
    from {
      opacity: 0;
    }
  }
  .label {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 18px 8px 6px;
    background: linear-gradient(transparent, rgba(4, 7, 15, 0.85));
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .badge {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--accent);
    color: var(--accent-ink);
    display: grid;
    place-items: center;
  }

  .error {
    color: var(--danger);
    margin: -6px 0 14px;
    font-size: 13px;
  }
  .none {
    color: var(--muted);
    padding: 8px 0;
  }
  .note {
    padding: 12px 20px;
    border-top: 1px solid var(--rule);
    color: var(--muted);
    font-size: 12px;
  }
</style>
