<script lang="ts">
  import { api } from '../lib/api'
  import Icon from '../lib/Icon.svelte'
  import { app, saveSettings } from '../lib/state.svelte'
  import Log from './Log.svelte'

  const s = $derived(app.status)
  const modes = [
    ['stay', 'Оставить'],
    ['minimize', 'Свернуть'],
    ['hide', 'Скрыть'],
  ] as const

  let size = $state<number | null>(null)
  let confirming = $state(false)
  let wipeError = $state('')
  let more = $state(!!app.settings.server || !!app.settings.game_args)

  $effect(() => {
    void api.dataSize().then((b) => (size = b))
  })

  function gb(bytes: number) {
    if (bytes < 1e9) return `${Math.max(1, Math.round(bytes / 1e6))} МБ`
    return `${(bytes / 1e9).toFixed(1).replace('.', ',')} ГБ`
  }

  async function wipe() {
    wipeError = ''
    try {
      await api.wipeData()
    } catch (e) {
      wipeError = String(e)
      confirming = false
    }
  }
</script>

{#if app.page === 'log'}
  <div class="wrap">
    <div class="top">
      <button class="back" onclick={() => (app.page = 'settings')}><span class="left"><Icon name="chevron" /></span> Настройки</button>
      <button class="x" aria-label="Закрыть" onclick={() => (app.page = 'play')}><Icon name="x" /></button>
    </div>
    <Log />
  </div>
{:else}
  <div class="wrap scroll">
    <div class="top">
      <h1>Настройки</h1>
      <button class="x" aria-label="Закрыть" onclick={() => (app.page = 'play')}><Icon name="x" /></button>
    </div>

    <h2>Игра</h2>
    <div class="item">
      <b>Окно лаунчера во время игры</b>
      <p>После выхода из игры окно вернётся само.</p>
      <div class="segment">
        {#each modes as [id, label]}
          <button class:on={app.settings.while_playing === id} onclick={() => ((app.settings.while_playing = id), saveSettings())}>{label}</button>
        {/each}
      </div>
    </div>

    <h2>Данные</h2>
    <div class="item">
      <div class="split">
        <div>
          <b>Занято на диске</b>
          <p>Игра, её ресурсы, ники, скины и настройки.</p>
        </div>
        <span class="size">{size === null ? '…' : gb(size)}</span>
      </div>
      <button class="btn" onclick={() => api.openDataDir()}><Icon name="folder" /> Открыть папку</button>
    </div>

    <div class="item danger-zone" class:alert={confirming}>
      <b>Удалить все данные</b>
      {#if confirming}
        <p class="warn">
          Удалится всё, что лаунчер скачал и сохранил{size ? ` (${gb(size)})` : ''}, и он закроется. Сам файл лаунчера останется, его
          удалите вручную.
        </p>
        <div class="pair">
          <button class="btn danger-solid" onclick={wipe}>Да, удалить всё</button>
          <button class="btn ghost" onclick={() => (confirming = false)}>Отмена</button>
        </div>
      {:else}
        <p>Стирает лаунчер с компьютера, как будто его не запускали.</p>
        <button class="btn danger" disabled={!!app.busy || app.playing} onclick={() => (confirming = true)}>
          <Icon name="trash" /> Удалить…
        </button>
      {/if}
      {#if wipeError}<p class="warn">{wipeError}</p>{/if}
    </div>

    <button class="more" onclick={() => (more = !more)} aria-expanded={more}>
      <span class="chev" class:open={more}><Icon name="chevron" size={16} /></span> Дополнительно
    </button>
    {#if more}
      <div class="item">
        <b>Журнал</b>
        <p>Что лаунчер делал в этом запуске. Пригодится, если что-то пошло не так.</p>
        <button class="btn" onclick={() => (app.page = 'log')}><Icon name="terminal" /> Открыть журнал</button>
      </div>
      <div class="item">
        <b>Адрес сервера</b>
        <p>Пусто — сервер WCraft. Меняйте только для проверки.</p>
        <input class="field" spellcheck="false" placeholder={s?.pack.server ?? ''} bind:value={app.settings.server} onchange={saveSettings} />
      </div>
      <div class="item">
        <b>Аргументы игры</b>
        <p>Через пробел. Обычно не нужны.</p>
        <input class="field" spellcheck="false" placeholder="--server-thread" bind:value={app.settings.game_args} onchange={saveSettings} />
      </div>
      <p class="versions">
        Лаунчер {s?.version ?? '…'} · Horizon Launcher {s?.cli ?? 'не установлен'} · Horizon {s?.pack.horizon_version ?? ''}
      </p>
    {/if}
  </div>
{/if}

<style>
  .wrap {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 18px 28px 28px;
  }
  .scroll {
    overflow-y: auto;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  h1 {
    font: 600 24px/1.2 var(--font);
    letter-spacing: -0.02em;
  }
  .x {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .x:hover {
    color: var(--ink);
    background: var(--surface-hi);
  }
  .back {
    height: 32px;
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--ink-soft);
    font-weight: 500;
  }
  .back:hover {
    color: var(--ink);
  }
  .left {
    display: flex;
    transform: rotate(90deg);
  }

  h2 {
    margin: 22px 0 8px;
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .item {
    padding: 14px 16px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--rule);
    display: grid;
    gap: 4px;
    justify-items: start;
  }
  .item + .item {
    margin-top: 8px;
  }
  .item b {
    font-weight: 500;
  }
  .item p {
    color: var(--muted);
    font-size: 13px;
  }
  .item > .segment,
  .item > .btn,
  .item > .field,
  .item > .pair {
    margin-top: 8px;
  }
  .item .field {
    width: 100%;
  }
  .split {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .size {
    font: 600 20px var(--font);
    font-variant-numeric: tabular-nums;
  }

  .danger-zone {
    transition: background 0.15s, border-color 0.15s;
  }
  .danger-zone.alert {
    background: color-mix(in oklab, var(--danger) 8%, transparent);
    border-color: color-mix(in oklab, var(--danger) 35%, transparent);
  }
  .item .warn {
    color: var(--danger);
  }
  .pair {
    display: flex;
    gap: 8px;
  }
  .danger-solid {
    background: #d94a4a;
    border-color: #d94a4a;
    color: #fff;
  }
  .danger-solid:hover:not(:disabled) {
    background: #e85b5b;
  }

  .more {
    margin: 24px 0 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ink-soft);
    font-weight: 500;
  }
  .more:hover {
    color: var(--ink);
  }
  .chev {
    display: flex;
    transform: rotate(-90deg);
    transition: transform 0.15s;
  }
  .chev.open {
    transform: none;
  }
  .versions {
    margin-top: 14px;
    color: var(--muted);
    font-size: 12px;
  }
</style>
