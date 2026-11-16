<script lang="ts">
  import { api } from '../lib/api'
  import Character from '../lib/Character.svelte'
  import Face from '../lib/Face.svelte'
  import AccountManage from '../lib/AccountManage.svelte'
  import Icon from '../lib/Icon.svelte'
  import { activeSkin, app, authenticate, installUpdate, play, removeAccount, selectAccount } from '../lib/state.svelte'

  /** What a first install downloads and keeps, measured on a clean machine (docs/horizon-findings.md). */
  const FIRST_INSTALL = '1,7 ГБ'
  const NICK = /^[a-zA-Z0-9_]{3,16}$/
  const gb = (bytes: number) => (bytes / 1e9).toLocaleString('ru', { maximumFractionDigits: 1, minimumFractionDigits: 1 }) + ' ГБ'

  /** The time setup should still take, rounded the way a person would say it. */
  function timeLeft(p: { done: number; total: number; eta: number | null }): string {
    if (p.done >= p.total * 0.97) return 'Почти готово'
    if (p.eta === null) return 'Оцениваю время…'
    const min = Math.round(p.eta / 60)
    if (min < 1) return 'Осталось меньше минуты'
    if (min < 60) return `Осталось около ${min} мин`
    return `Осталось около ${Math.floor(min / 60)} ч ${min % 60} мин`
  }

  const s = $derived(app.status)
  const ready = $derived(!!s && s.installed && s.prepared)
  const server = $derived(app.server)
  const online = $derived(server && server !== 'offline' ? server : null)
  const skin = $derived(activeSkin())

  // The player fills the height between the header and the dock.
  let winH = $state(740)
  const charH = $derived(Math.max(360, Math.min(600, winH - 185)))

  let nick = $state('')
  let password = $state('')
  /** The form registers a new account or logs in to one. */
  let register = $state(true)
  let nickError = $state('')
  let menu = $state(false)
  let adding = $state(false)
  let managing = $state(false)

  /**
   * The face shown by a name. Nicks of this computer wear the chosen skin (it goes to whichever
   * nick plays); other players the skin they published on the server, else a default one picked by the name.
   */
  function faceUrl(name: string): string | undefined {
    if (skin && s?.accounts.some((a) => a.name === name)) return skin.url
    if (app.players[name]) return app.players[name].url
    const defaults = app.skins?.defaults ?? []
    let h = 0
    for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0
    return defaults[h % defaults.length]?.url
  }

  async function submitAuth() {
    nickError = ''
    const name = nick.trim()
    if (!NICK.test(name)) {
      nickError = 'Ник: от 3 до 16 символов, латинские буквы, цифры и _'
      return
    }
    if (register && password.length < 6) {
      nickError = 'Пароль: не меньше 6 символов'
      return
    }
    await authenticate(register, name, password)
    if (app.error) {
      nickError = app.error
      app.error = ''
    } else {
      nick = ''
      password = ''
      adding = false
      menu = false
    }
  }

  function openForm(asRegister: boolean, name = '') {
    register = asRegister
    nick = name
    password = ''
    nickError = ''
    adding = true
  }

  async function pick(name: string) {
    menu = false
    await selectAccount(name)
  }
</script>

{#snippet face(name: string)}
  {@const url = faceUrl(name)}
  {#if url}<Face {url} size={18} />{:else}<i class="noface-sm"></i>{/if}
{/snippet}

{#snippet nickForm(autofocus: boolean)}
  <form
    class="auth"
    onsubmit={(e) => {
      e.preventDefault()
      void submitAuth()
    }}
  >
    <div class="modes" role="tablist">
      <button type="button" role="tab" aria-selected={register} class:on={register} onclick={() => ((register = true), (nickError = ''))}>Регистрация</button>
      <button type="button" role="tab" aria-selected={!register} class:on={!register} onclick={() => ((register = false), (nickError = ''))}>Вход</button>
    </div>
    <div class="nick">
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field nickname" placeholder="Никнейм" maxlength="16" spellcheck="false" autocomplete="username" {autofocus} bind:value={nick} disabled={!!app.busy} />
      <input class="field" type="password" placeholder="Пароль" maxlength="128" autocomplete={register ? 'new-password' : 'current-password'} bind:value={password} disabled={!!app.busy} />
      <button class="btn primary" type="submit" disabled={!nick.trim() || !password || !!app.busy}>
        {#if app.busy}<span class="spinner"></span>{:else}{register ? 'Создать' : 'Войти'}{/if}
      </button>
    </div>
  </form>
{/snippet}

<svelte:window bind:innerHeight={winH} />

{#if managing && s?.account}
  <AccountManage name={s.account.name} onclose={() => (managing = false)} />
{/if}

<div class="home">
  <section class="info">
    <h1>WCraft</h1>

    <div class="status" class:off={server === 'offline'}>
      {#if server === null}
        <i class="dot wait"></i> Проверяю сервер…
      {:else if server === 'offline'}
        <i class="dot"></i> Сервер недоступен
      {:else}
        <i class="dot"></i> В сети
        <span class="sep"></span>
        <b>{server.online}</b>&nbsp;из {server.max}
        <span class="sep"></span>
        {server.ping_ms} мс
      {/if}
    </div>

    {#if online?.players.length}
      <ul class="players">
        {#each online.players.slice(0, 5) as name}
          <li>{@render face(name)}<span class="nickname">{name}</span></li>
        {/each}
        {#if online.players.length > 5}<li class="more">ещё {online.players.length - 5}</li>{/if}
      </ul>
    {:else if online}
      <p class="quiet">Сейчас никого. Будь первым.</p>
    {/if}
  </section>

  <section class="stage">
    <Character {skin} name={s?.account?.name} still zoom={0.85} width={Math.round(charH * 0.7)} height={charH} />
    {#if s?.account}
      <button class="change" onclick={() => (app.page = 'skins')}><Icon name="shirt" size={15} /> {skin ? 'Сменить скин' : 'Выбрать скин'}</button>
    {/if}
  </section>

  <div class="dock">
    {#if !s}
      <div class="skeleton"></div>
    {:else if !s.account}
      <div class="first">
        <b>{register ? 'Создай аккаунт WCraft' : 'Войди в аккаунт WCraft'}</b>
        <span>{register ? 'Ник увидят другие игроки. Пароль защищает его от чужих.' : 'Ник и пароль, с которыми регистрировался.'}</span>
      </div>
      <div class="first-form">
        {@render nickForm(false)}
        {#if nickError}<p class="error">{nickError}</p>{/if}
      </div>
    {:else}
      <div class="who">
        <button class="acc-btn" onclick={() => (menu = !menu)} aria-expanded={menu} disabled={!!app.busy}>
          {#if skin}<Face url={skin.url} size={38} />{:else}<span class="noface"><Icon name="user" size={20} /></span>{/if}
          <span class="acc-text">
            <small>Играю как</small>
            <b>{s.account.name}</b>
          </span>
          <span class="chev" class:up={menu}><Icon name="chevron" size={16} /></span>
        </button>

        {#if menu}
          <button class="away" aria-label="Закрыть" onclick={() => ((menu = false), (adding = false))}></button>
          <div class="menu">
            <p class="menu-title">Ники на этом компьютере</p>
            {#each s.accounts as acc (acc.name)}
              <div class="acc" class:on={acc.name === s.account.name}>
                <button class="acc-pick" onclick={() => pick(acc.name)}>
                  {@render face(acc.name)}
                  <span class="nickname">{acc.name}</span>
                  {#if !s.logged_in.includes(acc.name)}<small class="out">не вошёл</small>{/if}
                  {#if acc.name === s.account.name}<Icon name="check" size={15} />{/if}
                </button>
                <button
                  class="acc-x"
                  title="Убрать из списка"
                  aria-label="Убрать {acc.name}"
                  disabled={!!app.busy || app.playing}
                  onclick={() => removeAccount(acc.name, acc.offline)}><Icon name="x" size={14} /></button
                >
              </div>
            {/each}
            <div class="menu-foot">
              {#if adding}
                {@render nickForm(true)}
                {#if nickError}<p class="error small">{nickError}</p>{/if}
              {:else}
                {#if !s.logged_in.includes(s.account.name)}
                  <button class="menu-link" onclick={() => openForm(false, s.account!.name)}>
                    <Icon name="user" size={15} /> Войти как {s.account.name}
                  </button>
                {/if}
                {#if s.logged_in.includes(s.account.name)}
                  <button class="menu-link" onclick={() => ((managing = true), (menu = false))}>
                    <Icon name="sliders" size={15} /> Управление аккаунтом
                  </button>
                {/if}
                <button class="menu-link" onclick={() => openForm(false)}>
                  <Icon name="plus" size={15} /> Другой аккаунт
                </button>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <div class="middle">
        {#if app.busy}
          <p class="step">{app.step}</p>
          {#if app.progress}
            {@const p = app.progress}
            <div class="progress"><b style:width="{Math.min(99, (p.done / p.total) * 100)}%"></b></div>
            <p class="line">{timeLeft(p)} · {gb(Math.min(p.done, p.total))} из {gb(p.total)}</p>
          {:else}
            <div class="progress"><i></i></div>
            <p class="line">{app.lastLine || 'Это может занять несколько минут'}</p>
          {/if}
        {:else if app.error}
          <p class="error clip" title={app.error}>{app.error}</p>
          <button class="menu-link" onclick={() => (app.page = 'log')}>Подробности в журнале</button>
        {:else if app.updating !== null}
          <p class="step">Обновляю лаунчер до {app.update?.version}</p>
          <div class="progress"><b style:width="{app.updating}%"></b></div>
          <p class="line">После загрузки лаунчер перезапустится сам</p>
        {:else if app.playing}
          <p class="hint">Игра открыта в отдельном окне. Лаунчер вернётся, когда выйдешь.</p>
        {:else if !ready}
          <p class="hint">Первый запуск скачает игру, около {FIRST_INSTALL}.<br />Потом запуск занимает секунды.</p>
        {:else if app.update}
          <p class="hint">Вышла версия лаунчера {app.update.version}. Играть можно только на ней:<br />обновление
            {Math.round(app.update.size / 1e6)} МБ, лаунчер перезапустится сам.</p>
        {:else if server === 'offline'}
          <p class="hint">Сервер не отвечает. Попробуй чуть позже или напиши администратору.</p>
        {:else}
          <p class="hint">Всё готово. Версия {s.pack.horizon_version.replace(/^nightly-/, '')}</p>
        {/if}
      </div>

      <!-- While setting up, the same button cancels. -->
      {#if app.update && !app.busy && !app.playing}
        <button class="play" disabled={app.updating !== null} onclick={installUpdate}>
          {#if app.updating === null}
            <Icon name="download" size={20} /> Обновить
          {:else}
            <span class="spinner"></span> {app.updating}%
          {/if}
        </button>
      {:else}
      <button class="play" class:live={app.playing} class:busy={!!app.busy} disabled={app.playing} onclick={() => (app.busy ? api.cancel() : play())}>
        {#if app.busy}
          <span class="spinner"></span> Отменить
        {:else if app.playing}
          <i class="dot"></i> В игре
        {:else}
          <Icon name="play" size={20} /> {ready ? 'Играть' : 'Установить'}
        {/if}
      </button>
      {/if}
    {/if}
  </div>
</div>

<style>
  .home {
    height: 100%;
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr auto;
    padding: 0 28px 24px;
  }

  /* Server */
  .info {
    align-self: center;
    padding: 0 0 40px 36px;
  }
  h1 {
    font: 700 104px/0.95 var(--pixel);
    letter-spacing: 0.01em;
    color: #f4f1e6;
    text-shadow: 6px 6px 0 #1c2b55;
  }
  .status {
    margin-top: 26px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 14px;
    border-radius: 6px;
    background: rgba(6, 10, 22, 0.55);
    border: 1px solid var(--stroke);
    color: var(--ink-soft);
  }
  .status b {
    color: var(--ink);
    font-weight: 600;
  }
  .status .sep {
    width: 1px;
    height: 14px;
    margin: 0 4px;
    background: var(--stroke);
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    background: var(--live);
    box-shadow: 0 0 10px var(--live);
  }
  .off .dot {
    background: var(--danger);
    box-shadow: 0 0 10px var(--danger);
  }
  .dot.wait {
    background: var(--muted);
    box-shadow: none;
  }
  .nickname {
    font-family: var(--nick);
  }
  .players {
    margin: 14px 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    max-width: 440px;
  }
  .players li {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 10px 0 6px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    font-size: 13px;
  }
  .noface-sm {
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 3px;
    background: var(--surface-hi);
  }
  .players :global(.face),
  .acc-pick :global(.face) {
    border-radius: 3px;
  }
  .players .more {
    padding-left: 10px;
    color: var(--muted);
  }
  .quiet {
    margin-top: 14px;
    color: var(--muted);
  }

  /* The player */
  .stage {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    padding-bottom: 8px;
    /* The player may reach up behind the transparent header. */
    margin-top: -16px;
  }
  .change {
    height: 30px;
    padding: 0 12px;
    margin-top: -4px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--ink-soft);
    font-size: 13px;
    font-weight: 500;
  }
  .change:hover {
    color: var(--ink);
    background: rgba(255, 255, 255, 0.07);
  }

  /* Bottom dock */
  .dock {
    grid-column: 1 / -1;
    position: relative;
    z-index: 2;
    height: 88px;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 14px 0 14px;
    border-radius: 14px;
    background: rgba(9, 13, 28, 0.88);
    border: 1px solid var(--stroke);
    box-shadow: 0 20px 50px -20px rgba(0, 0, 0, 0.8);
    backdrop-filter: blur(14px);
  }
  .skeleton {
    flex: 1;
    height: 60px;
    border-radius: 10px;
    background: var(--surface-hi);
  }

  .who {
    position: relative;
  }
  .acc-btn {
    height: 60px;
    padding: 0 12px 0 11px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
    transition: background 0.12s;
  }
  .acc-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.06);
  }
  .noface {
    width: 38px;
    height: 38px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    background: var(--surface-hi);
    color: var(--muted);
  }
  .acc-text {
    display: grid;
    line-height: 1.2;
  }
  .acc-text small {
    color: var(--muted);
    font-size: 12px;
  }
  .acc-text b {
    font: 600 17px/1.2 var(--nick);
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chev {
    display: flex;
    color: var(--muted);
    transition: transform 0.15s;
  }
  .chev.up {
    transform: rotate(180deg);
  }

  .away {
    position: fixed;
    inset: 0;
    z-index: 5;
    cursor: default;
  }
  .menu {
    position: absolute;
    z-index: 6;
    bottom: calc(100% + 14px);
    left: 0;
    width: 290px;
    padding: 8px;
    border-radius: 12px;
    background: var(--panel);
    border: 1px solid var(--stroke);
    box-shadow: 0 20px 50px -12px rgba(0, 0, 0, 0.8);
    animation: pop 0.14s ease-out;
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  .menu-title {
    padding: 4px 8px 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .acc {
    display: flex;
    align-items: center;
    border-radius: 8px;
  }
  .acc:hover,
  .acc.on {
    background: var(--surface-hi);
  }
  .acc-pick {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
  }
  .acc-pick span {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-right: auto;
  }
  .acc-x {
    width: 30px;
    height: 30px;
    margin-right: 3px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--muted);
    opacity: 0;
  }
  .acc:hover .acc-x {
    opacity: 1;
  }
  .acc-x:hover {
    color: var(--danger);
    background: rgba(255, 255, 255, 0.06);
  }
  .menu-foot {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--rule);
    display: grid;
    gap: 2px;
  }
  .menu-foot .auth {
    padding: 4px;
    display: grid;
    gap: 8px;
  }
  .menu-foot .nick {
    flex-direction: column;
  }
  .menu-foot .field {
    width: 100%;
    min-width: 0;
  }
  .menu-foot .error {
    padding: 0 6px 4px;
  }

  .menu-link {
    height: 30px;
    padding: 0 8px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--accent);
    font-weight: 500;
    text-align: left;
  }
  .menu-link:hover:not(:disabled) {
    background: var(--surface-hi);
  }

  .middle {
    flex: 1;
    min-width: 0;
    padding-left: 18px;
    border-left: 1px solid var(--rule);
  }
  .hint {
    color: var(--muted);
    font-size: 13px;
    line-height: 1.45;
  }
  .step {
    font-weight: 500;
  }
  .progress {
    height: 4px;
    margin-top: 8px;
    background: var(--surface-hi);
    overflow: hidden;
  }
  .progress i {
    display: block;
    width: 35%;
    height: 100%;
    background: var(--accent);
    animation: run 1.4s ease-in-out infinite;
  }
  .progress b {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.9s linear;
  }
  @keyframes run {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }
  .line {
    margin-top: 6px;
    color: var(--muted);
    font: 12px var(--mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .error {
    color: var(--danger);
    font-size: 13px;
    user-select: text;
    -webkit-user-select: text;
  }
  .clip {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .error + .menu-link {
    margin-left: -8px;
  }
  .small {
    font-size: 12px;
  }

  .play {
    min-width: 230px;
    height: 62px;
    padding: 0 28px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-ink);
    font: 700 20px var(--font);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    box-shadow:
      inset 0 -5px 0 rgba(0, 0, 0, 0.22),
      inset 0 2px 0 rgba(255, 255, 255, 0.35),
      0 10px 30px -8px color-mix(in srgb, var(--accent) 60%, transparent);
    transition: transform 0.08s, filter 0.12s;
  }
  .play:hover:not(:disabled) {
    filter: brightness(1.07);
  }
  .play:active:not(:disabled) {
    transform: translateY(2px);
    box-shadow:
      inset 0 -2px 0 rgba(0, 0, 0, 0.22),
      inset 0 2px 0 rgba(255, 255, 255, 0.35);
  }
  .play.busy {
    background: var(--surface-hi);
    color: var(--ink-soft);
    font: 500 16px var(--font);
    box-shadow: inset 0 0 0 1px var(--stroke);
  }
  .play.busy {
    transition: none;
  }
  .play.busy:hover {
    color: var(--danger);
    filter: none;
  }
  .play.live {
    background: color-mix(in srgb, var(--live) 14%, transparent);
    color: var(--live);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--live) 40%, transparent);
  }

  .first {
    display: grid;
    gap: 2px;
    padding-left: 12px;
    margin-right: auto;
  }
  .first b {
    font-size: 18px;
    font-weight: 600;
  }
  .first span {
    color: var(--muted);
    font-size: 13px;
  }
  .first-form {
    position: relative;
  }
  .first-form .error {
    position: absolute;
    bottom: calc(100% + 26px);
    left: 0;
    white-space: nowrap;
  }
  .first-form .modes {
    position: absolute;
    bottom: calc(100% + 26px);
    right: 0;
  }
  .modes {
    display: inline-flex;
    justify-self: start;
    padding: 3px;
    gap: 2px;
    border-radius: 8px;
    background: var(--rule);
  }
  .modes button {
    height: 26px;
    padding: 0 12px;
    border-radius: 6px;
    color: var(--muted);
    font: 500 13px var(--font);
  }
  .modes button.on {
    background: var(--panel);
    color: var(--ink);
  }
  .out {
    margin-left: auto;
    color: var(--muted);
    font-size: 11px;
  }
  .nick {
    display: flex;
    gap: 8px;
  }
  .first-form .field {
    width: 200px;
    height: 54px;
    font: 500 16px var(--font);
  }
  .first-form .btn {
    height: 54px;
    padding: 0 22px;
    font-size: 15px;
  }
</style>
