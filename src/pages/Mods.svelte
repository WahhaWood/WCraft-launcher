<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type Mod, type ModsState } from '../lib/api'
  import Icon from '../lib/Icon.svelte'
  import { app, buildMods } from '../lib/state.svelte'

  let picker: HTMLInputElement
  let mods = $state<ModsState | null>(null)
  let failed = $state('')
  let tab = $state<'mine' | 'ours'>('mine')
  /** The mod an action runs on; '' for an added file. */
  let busy = $state<string | null>(null)
  let error = $state('')
  let errorAt = $state<string | null>(null)
  let dragging = $state(false)
  let cleaning = $state(false)
  let confirmClean = $state(false)
  /** Seconds the build has been running, ticking while it does. */
  let now = $state(Date.now())

  const building = $derived(app.busy === 'build')
  const locked = $derived(app.playing || !!app.busy || busy !== null)
  const on = $derived(mods?.mine.filter((m) => m.enabled).length ?? 0)

  async function load() {
    try {
      mods = await api.modsState()
      failed = ''
    } catch (e) {
      failed = String(e)
    }
  }

  onMount(() => {
    void load()
    const timer = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(timer)
  })

  async function act(id: string, run: () => Promise<unknown>) {
    busy = id
    error = ''
    errorAt = null
    try {
      await run()
      await load()
    } catch (e) {
      error = String(e)
      errorAt = id
    } finally {
      busy = null
    }
  }

  async function addFile(file: File | undefined) {
    if (!file || locked) return
    await act('', async () => {
      const id = await api.modsAdd(file)
      tab = 'mine'
      return id
    })
  }

  function drop(e: DragEvent) {
    e.preventDefault()
    dragging = false
    void addFile(e.dataTransfer?.files[0])
  }

  async function build() {
    app.error = ''
    await buildMods()
    await load()
  }

  async function clean() {
    cleaning = true
    try {
      await api.modsClean()
      await load()
    } catch (e) {
      app.error = String(e)
    } finally {
      cleaning = false
      confirmClean = false
    }
  }

  function clock(ms: number) {
    const s = Math.max(0, Math.floor(ms / 1000))
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
  }

  function gb(bytes: number) {
    if (bytes < 1e9) return `${Math.max(1, Math.round(bytes / 1e6))} МБ`
    return `${(bytes / 1e9).toFixed(1).replace('.', ',')} ГБ`
  }

  const headline = $derived.by(() => {
    if (!mods) return { icon: 'cube', tone: '', title: 'Загружаю…', text: '' }
    if (!mods.sources)
      return {
        icon: 'alert',
        tone: 'warn',
        title: 'Сборка пока недоступна',
        text: 'Сервер ещё не выложил исходники модов WCraft, без них игру не собрать. Моды можно добавить заранее.',
      }
    switch (mods.build) {
      case 'none':
        return {
          icon: 'cube',
          tone: '',
          title: 'Своих модов нет',
          text: 'Добавь мод справа и включи его. Пока игра запускается в обычной сборке WCraft.',
        }
      case 'missing':
        return {
          icon: 'hammer',
          tone: 'warn',
          title: 'Нужна сборка',
          text: `Игру нужно собрать с ${plural(on, 'твоим модом', 'твоими модами', 'твоими модами')}. Пока «Играть» запускает обычную сборку WCraft.`,
        }
      case 'stale':
        return {
          icon: 'hammer',
          tone: 'warn',
          title: 'Нужно пересобрать',
          text: 'Моды или сборка WCraft поменялись после прошлой сборки. Пока «Играть» запускает обычную сборку без твоих модов.',
        }
      default:
        return {
          icon: 'check',
          tone: 'ok',
          title: 'Собрано',
          text: `«Играть» запустит игру с модами WCraft и ${plural(on, 'твоим модом', 'твоими модами', 'твоими модами')}.`,
        }
    }
  })

  function plural(n: number, one: string, few: string, many: string) {
    const word = n % 10 === 1 && n % 100 !== 11 ? one : n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 10 || n % 100 >= 20) ? few : many
    return n === 1 ? word : `${n} ${word}`
  }

  const canBuild = $derived(!!mods?.sources && on > 0 && !app.playing && !app.busy)
  const list = $derived<Mod[]>(tab === 'mine' ? (mods?.mine ?? []) : (mods?.ours ?? []))
</script>

<div
  class="mods"
  role="region"
  aria-label="Моды"
  ondragover={(e) => {
    e.preventDefault()
    dragging = true
  }}
  ondragleave={(e) => !e.relatedTarget && (dragging = false)}
  ondrop={drop}
>
  <section class="stage">
    <button class="back" onclick={() => (app.page = 'play')}><span class="left"><Icon name="chevron" /></span> На главную</button>

    <div class="body">
      <div class="head">
        <span class="exp"><Icon name="flask" size={14} /> Экспериментально</span>
        <h1 class="page-title">Сборка с модами</h1>
        <p class="page-sub">Лаунчер собирает игру на твоём компьютере: моды WCraft всегда внутри, твои добавляются к ним.</p>
      </div>

      <div class="card {building ? 'run' : headline.tone}">
        {#if building}
          <div class="state">
            <span class="badge"><span class="spinner"></span></span>
            <div class="words">
              <b>{app.step}</b>
              <p class="mono">{app.lastLine || 'Это займёт время'}</p>
            </div>
            <span class="timer">{clock(now - app.since)}</span>
          </div>
          {#if app.progress}
            <div class="progress"><b style:width="{Math.min(99, (app.progress.done / app.progress.total) * 100)}%"></b></div>
          {:else}
            <div class="progress"><i></i></div>
          {/if}
          <div class="actions">
            <button class="btn" onclick={() => api.cancel()}><Icon name="x" /> Отменить</button>
            <button class="btn ghost" onclick={() => (app.page = 'log')}><Icon name="terminal" /> Журнал</button>
            <span class="hint">Можно свернуть лаунчер: сборка идёт с низким приоритетом.</span>
          </div>
        {:else}
          <div class="state">
            <span class="badge"><Icon name={headline.icon as 'cube'} size={22} /></span>
            <div class="words">
              <b>{headline.title}</b>
              <p>{headline.text}</p>
            </div>
          </div>
          {#if app.error}
            <div class="fail">
              <p>{app.error}</p>
              <button class="btn ghost" onclick={() => (app.page = 'log')}><Icon name="terminal" /> Журнал</button>
            </div>
          {/if}
          {#if mods && mods.build !== 'none' && mods.sources}
            <div class="actions">
              {#if mods.build === 'ready'}
                <button class="btn primary" disabled={app.playing} onclick={() => (app.page = 'play')}><Icon name="play" /> К игре</button>
                <button class="btn ghost" disabled={!canBuild} onclick={build}><Icon name="hammer" /> Пересобрать</button>
              {:else}
                <button class="btn primary" disabled={!canBuild} onclick={build}>
                  <Icon name="hammer" />
                  {mods.build === 'stale' ? 'Пересобрать' : 'Собрать'}
                </button>
                {#if app.playing}<span class="hint">Собрать можно, когда игра закрыта.</span>{/if}
              {/if}
            </div>
          {/if}
        {/if}
      </div>

      <div class="facts">
        <div class="fact">
          <Icon name="alert" />
          <p><b>Без песочницы.</b> Код мода получает те же права, что и ты. Ставь только моды, которым доверяешь.</p>
        </div>
        <div class="fact">
          <Icon name="cube" />
          <p><b>Только клиент.</b> Сервер WCraft не меняется: мод, которому нужна серверная часть, на нём не заработает.</p>
        </div>
        <div class="fact">
          <Icon name="hammer" />
          <p>
            <b>Первая сборка 10–30 минут, около 8 ГБ.</b> Лаунчер скачает Rust и исходники игры, дальше пересборка быстрее.
            {#if mods?.toolchain_size}Сейчас занято {gb(mods.toolchain_size)}.{/if}
          </p>
          {#if mods?.toolchain_size}
            {#if confirmClean}
              <span class="pair">
                <button class="btn danger" disabled={cleaning || locked} onclick={clean}>{#if cleaning}<span class="spinner"></span>{/if} Удалить</button>
                <button class="btn ghost" onclick={() => (confirmClean = false)}>Нет</button>
              </span>
            {:else}
              <button class="btn ghost" title="Удалить Rust, кэш и собранную игру. Твои моды останутся." disabled={locked} onclick={() => (confirmClean = true)}>
                <Icon name="trash" /> Освободить
              </button>
            {/if}
          {/if}
        </div>
      </div>
    </div>
  </section>

  <aside class="panel">
    <div class="top">
      <h1>Моды</h1>
      <div class="segment">
        <button class:on={tab === 'mine'} onclick={() => (tab = 'mine')}>Мои <small>{mods?.mine.length ?? 0}</small></button>
        <button class:on={tab === 'ours'} onclick={() => (tab = 'ours')}>WCraft <small>{mods?.ours.length ?? 0}</small></button>
      </div>
    </div>

    <div class="scroll">
      {#if failed}
        <p class="none">{failed}</p>
      {:else if !mods}
        <p class="none loading"><span class="spinner"></span> Загружаю…</p>
      {:else}
        {#if tab === 'mine'}
          <button class="drop" class:over={dragging} disabled={locked} onclick={() => picker.click()}>
            <span class="drop-icon">{#if busy === ''}<span class="spinner dark"></span>{:else}<Icon name="plus" size={20} />{/if}</span>
            <span>
              <b>Добавить мод</b>
              <small>.hnmod, .hmod или .zip, перетащи файл сюда или нажми</small>
            </span>
          </button>
          {#if error && errorAt === ''}<p class="error">{error}</p>{/if}
        {:else}
          <p class="lead"><Icon name="lock" size={14} /> Обязательны для сервера WCraft, всегда в сборке.</p>
        {/if}

        {#each list as m (m.id)}
          <div class="line">
            <div class="row" class:off={!m.enabled}>
              <span class="pic"><Icon name={tab === 'ours' ? 'lock' : 'cube'} size={18} /></span>
              <span class="text">
                <b>{m.id}</b>
                <small>{m.version ? `версия ${m.version}` : 'без версии'}</small>
              </span>
            </div>
            {#if tab === 'mine'}
              <button
                class="remove"
                aria-label="Удалить"
                title="Удалить"
                disabled={locked}
                onclick={() => act(m.id, () => api.modsRemove(m.id))}
              >
                <Icon name="trash" />
              </button>
              <button
                class="switch"
                class:on={m.enabled}
                role="switch"
                aria-checked={m.enabled}
                aria-label={m.enabled ? 'Выключить' : 'Включить'}
                title={m.enabled ? 'Выключить' : 'Включить'}
                disabled={locked}
                onclick={() => act(m.id, () => api.modsSet(m.id, !m.enabled))}
              >
                {#if busy === m.id}<span class="spinner"></span>{:else}<span class="knob"></span>{/if}
              </button>
            {/if}
          </div>
          {#if error && errorAt === m.id}<p class="error">{error}</p>{/if}
        {:else}
          {#if tab === 'mine'}
            <p class="none">Своих модов пока нет. Нативные моды Horizon лежат в каталоге модов Horizon и на GitHub авторов.</p>
          {:else}
            <p class="none">Сборка WCraft без модов.</p>
          {/if}
        {/each}
      {/if}
    </div>

    <div class="note">
      {#if app.playing}
        Игра запущена: изменения войдут в следующую сборку.
      {:else}
        Включение и выключение мода требует пересборки.
        <button class="folder" onclick={() => api.modsOpenDir().catch((e) => ((error = String(e)), (errorAt = '')))}><Icon name="folder" size={14} /> Папка</button>
      {/if}
    </div>
  </aside>

  <input bind:this={picker} type="file" accept=".hnmod,.hmod,.zip" hidden onchange={() => (addFile(picker.files?.[0]), (picker.value = ''))} />
</div>

<style>
  .mods {
    display: grid;
    grid-template-columns: 1fr 420px;
    height: 100%;
  }

  .stage {
    position: relative;
    display: flex;
    flex-direction: column;
    padding: 48px 28px 24px 24px;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
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
  .body {
    width: 100%;
    max-width: 640px;
    margin: auto;
    display: grid;
    gap: 18px;
  }
  .exp {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    margin-bottom: 12px;
    border-radius: 99px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    font-size: 12px;
    font-weight: 600;
  }
  .page-sub {
    max-width: 520px;
  }

  .card {
    padding: 18px;
    border-radius: 14px;
    background: rgba(9, 13, 28, 0.9);
    border: 1px solid var(--stroke);
    box-shadow: 0 20px 50px -20px rgba(0, 0, 0, 0.8);
    display: grid;
    gap: 14px;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .badge {
    width: 46px;
    height: 46px;
    flex: none;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: var(--surface-hi);
    color: var(--ink-soft);
  }
  .card.warn .badge {
    background: color-mix(in srgb, var(--warn) 16%, transparent);
    color: var(--warn);
  }
  .card.ok .badge {
    background: color-mix(in srgb, var(--live) 16%, transparent);
    color: var(--live);
  }
  .card.run .badge {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .words {
    flex: 1;
    min-width: 0;
  }
  .words b {
    font: 600 17px/1.3 var(--font);
  }
  .words p {
    color: var(--muted);
    font-size: 13px;
  }
  .words .mono {
    font: 12px var(--mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .timer {
    font: 600 20px var(--font);
    font-variant-numeric: tabular-nums;
    color: var(--ink-soft);
  }
  .progress {
    height: 6px;
    border-radius: 99px;
    background: var(--surface-hi);
    overflow: hidden;
    position: relative;
  }
  .progress b {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.4s;
  }
  .progress i {
    position: absolute;
    inset: 0 auto 0 0;
    width: 30%;
    background: var(--accent);
    border-radius: 99px;
    animation: slide 1.4s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .fail {
    padding: 10px 12px;
    border-radius: 10px;
    background: color-mix(in oklab, var(--danger) 10%, transparent);
    border: 1px solid color-mix(in oklab, var(--danger) 30%, transparent);
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .fail p {
    flex: 1;
    color: var(--danger);
    font-size: 13px;
    white-space: pre-wrap;
    user-select: text;
    -webkit-user-select: text;
  }

  .facts {
    display: grid;
    gap: 8px;
  }
  .fact {
    padding: 10px 14px;
    border-radius: 12px;
    background: rgba(9, 13, 28, 0.75);
    border: 1px solid var(--rule);
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--muted);
  }
  .fact p {
    flex: 1;
    font-size: 13px;
  }
  .fact b {
    color: var(--ink);
    font-weight: 600;
  }
  .pair {
    display: flex;
    gap: 4px;
  }

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
  .top h1 {
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
    padding: 2px 12px 12px;
  }
  .lead {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 10px;
    color: var(--muted);
    font-size: 13px;
  }

  .line {
    position: relative;
    display: flex;
    align-items: center;
  }
  .row {
    width: 100%;
    padding: 8px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    gap: 12px;
    transition: background 0.12s, opacity 0.12s;
  }
  .line:hover .row {
    background: var(--surface-hi);
  }
  .line:has(.switch) .row {
    padding-right: 96px;
  }
  .row.off {
    opacity: 0.55;
  }
  .pic {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: linear-gradient(180deg, #1b2a4f, #111a31);
    color: var(--muted);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .text b {
    font: 600 14px var(--mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text small {
    color: var(--muted);
    font-size: 12px;
  }
  .remove {
    position: absolute;
    right: 56px;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--muted);
    opacity: 0;
    transition: opacity 0.12s, color 0.12s, background 0.12s;
  }
  .line:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }
  .remove:hover:not(:disabled) {
    color: var(--danger);
    background: var(--surface-hi);
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
  .switch:disabled,
  .remove:disabled {
    opacity: 0.5;
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
  .spinner.dark {
    border-color: rgba(0, 0, 0, 0.2);
    border-top-color: var(--accent-ink);
  }
  .error {
    color: var(--danger);
    font-size: 13px;
    padding: 2px 8px 8px;
  }
  .none {
    color: var(--muted);
    padding: 8px;
    font-size: 13px;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .note {
    padding: 10px 12px 10px 20px;
    border-top: 1px solid var(--rule);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    color: var(--muted);
    font-size: 12px;
  }
  .folder {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: 6px;
    color: var(--ink-soft);
    flex: none;
  }
  .folder:hover {
    color: var(--ink);
    background: var(--surface-hi);
  }
</style>
