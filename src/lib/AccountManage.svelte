<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from './api'
  import Icon from './Icon.svelte'
  import { app, refresh } from './state.svelte'

  let { name, onclose }: { name: string; onclose: () => void } = $props()

  const NICK = /^[a-zA-Z0-9_]{3,16}$/
  type Section = 'nick' | 'password' | 'delete' | null
  let open = $state<Section>(null)
  let busy = $state(false)
  let error = $state('')
  let done = $state('')
  let created = $state(0)
  let renamed = $state<number | null>(null)

  let newNick = $state('')
  let password = $state('')
  let newPassword = $state('')
  let repeat = $state('')
  let sure = $state(false)

  const locked = $derived(busy || app.playing)
  const date = (t: number) => new Date(t * 1000).toLocaleDateString('ru')
  /** Hours until the name may change again; 0 when it may. */
  const wait = $derived(renamed === null ? 0 : Math.max(0, Math.ceil((renamed + 24 * 3600 - Date.now() / 1000) / 3600)))

  onMount(async () => {
    try {
      ;[created, renamed] = await api.accountInfo(name)
    } catch {
      // Not logged in or no connection: the actions say so themselves.
    }
  })

  function toggle(s: Section) {
    open = open === s ? null : s
    error = ''
    done = ''
    password = newPassword = repeat = ''
    sure = false
  }

  /** Runs one change; the service's words stand as the error. */
  async function run(change: () => Promise<string | void>) {
    if (locked) return
    busy = true
    error = ''
    done = ''
    try {
      done = (await change()) ?? ''
    } catch (e) {
      error = String(e)
      return false
    } finally {
      busy = false
    }
    return true
  }

  async function rename() {
    if (!NICK.test(newNick.trim())) return void (error = 'Ник: от 3 до 16 символов, латиница, цифры и _')
    const ok = await run(async () => {
      const now = await api.accountRename(name, password, newNick.trim())
      await refresh()
      return `Теперь ты ${now}. Инвентарь и прогресс остались при тебе.`
    })
    if (ok) onclose()
  }

  async function changePassword() {
    if (newPassword.length < 6) return void (error = 'Пароль: не меньше 6 символов')
    if (newPassword !== repeat) return void (error = 'Пароли не совпадают')
    const ok = await run(async () => {
      await api.accountPassword(name, password, newPassword)
      return 'Пароль изменён. На других компьютерах нужно войти заново.'
    })
    if (ok) toggle(null)
  }

  async function others() {
    await run(async () => {
      const n = await api.accountLogoutOthers(name)
      return n ? `Вышли на других устройствах: ${n}` : 'Других входов нет'
    })
  }

  async function remove() {
    const ok = await run(async () => {
      await api.accountDelete(name, password)
      await refresh()
    })
    if (ok) onclose()
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !busy && onclose()} />

<button class="scrim" aria-label="Закрыть" onclick={() => !busy && onclose()}></button>
<div class="sheet" role="dialog" aria-label="Управление аккаунтом">
  <header>
    <div>
      <small>Аккаунт</small>
      <h2>{name}</h2>
      {#if created}<p class="muted">Создан {date(created)}</p>{/if}
    </div>
    <button class="x" aria-label="Закрыть" onclick={onclose}><Icon name="x" size={16} /></button>
  </header>

  {#if app.playing}<p class="note">Игра запущена: аккаунт меняется, пока она закрыта.</p>{/if}

  <section>
    <button class="head" onclick={() => toggle('nick')} aria-expanded={open === 'nick'}>
      <span><b>Сменить ник</b><small>Инвентарь, достижения и статистика переедут на новый ник</small></span>
      <span class="chev" class:up={open === 'nick'}><Icon name="chevron" size={16} /></span>
    </button>
    {#if open === 'nick'}
      <form onsubmit={(e) => (e.preventDefault(), rename())}>
        {#if wait > 0}
          <p class="note">Ник можно менять раз в сутки, подожди ещё {wait} ч.</p>
        {:else}
          <p class="hint">Перед сменой выйди с сервера. Прежний ник освободится для других.</p>
          <input class="field nick" placeholder="Новый ник" maxlength="16" spellcheck="false" bind:value={newNick} disabled={locked} />
          <input class="field" type="password" placeholder="Пароль" autocomplete="current-password" bind:value={password} disabled={locked} />
          <button class="btn primary" type="submit" disabled={locked || !newNick.trim() || !password}>
            {#if busy}<span class="spinner dark"></span>{/if} Сменить ник
          </button>
        {/if}
      </form>
    {/if}
  </section>

  <section>
    <button class="head" onclick={() => toggle('password')} aria-expanded={open === 'password'}>
      <span><b>Сменить пароль</b><small>На других компьютерах придётся войти заново</small></span>
      <span class="chev" class:up={open === 'password'}><Icon name="chevron" size={16} /></span>
    </button>
    {#if open === 'password'}
      <form onsubmit={(e) => (e.preventDefault(), changePassword())}>
        <input class="field" type="password" placeholder="Текущий пароль" autocomplete="current-password" bind:value={password} disabled={locked} />
        <input class="field" type="password" placeholder="Новый пароль" maxlength="128" autocomplete="new-password" bind:value={newPassword} disabled={locked} />
        <input class="field" type="password" placeholder="Новый пароль ещё раз" maxlength="128" autocomplete="new-password" bind:value={repeat} disabled={locked} />
        <button class="btn primary" type="submit" disabled={locked || !password || !newPassword || !repeat}>
          {#if busy}<span class="spinner dark"></span>{/if} Сменить пароль
        </button>
      </form>
    {/if}
  </section>

  <section>
    <button class="head" onclick={others} disabled={locked}>
      <span><b>Выйти на других устройствах</b><small>Если заходил с чужого компьютера</small></span>
      {#if busy && !open}<span class="spinner"></span>{/if}
    </button>
  </section>

  <section>
    <button class="head danger" onclick={() => toggle('delete')} aria-expanded={open === 'delete'}>
      <span><b>Удалить аккаунт</b><small>Ник освободится. Нельзя отменить</small></span>
      <span class="chev" class:up={open === 'delete'}><Icon name="chevron" size={16} /></span>
    </button>
    {#if open === 'delete'}
      <form onsubmit={(e) => (e.preventDefault(), remove())}>
        <p class="hint">Аккаунт, скин и вход на сервер пропадут. Место на этом компьютере освободится только через 7 дней.</p>
        <input class="field" type="password" placeholder="Пароль" autocomplete="current-password" bind:value={password} disabled={locked} />
        <label class="sure"><input type="checkbox" bind:checked={sure} disabled={locked} /> Я понимаю, что это навсегда</label>
        <button class="btn danger-solid" type="submit" disabled={locked || !password || !sure}>
          {#if busy}<span class="spinner"></span>{/if} Удалить аккаунт {name}
        </button>
      </form>
    {/if}
  </section>

  {#if error}<p class="error">{error}</p>{/if}
  {#if done}<p class="done">{done}</p>{/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgba(4, 7, 16, 0.6);
    backdrop-filter: blur(3px);
  }
  .sheet {
    position: fixed;
    z-index: 41;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(420px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    overflow-y: auto;
    padding: 18px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--stroke);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 6px;
  }
  header small {
    color: var(--muted);
  }
  h2 {
    margin: 0;
    font-family: var(--nick);
    font-size: 20px;
  }
  .muted {
    margin: 2px 0 0;
    color: var(--muted);
    font-size: 12px;
  }
  .x {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    color: var(--ink-soft);
  }
  .x:hover {
    background: var(--surface-hi);
  }
  section {
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--rule);
    overflow: hidden;
  }
  .head {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 11px 13px;
    text-align: left;
  }
  .head:hover:not(:disabled) {
    background: var(--surface-hi);
  }
  .head span:first-child {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .head small {
    color: var(--muted);
    font-size: 12px;
  }
  .head.danger b {
    color: var(--danger);
  }
  .chev {
    display: flex;
    transition: transform 0.15s;
  }
  .chev.up {
    transform: rotate(180deg);
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0 13px 13px;
  }
  .nick {
    font-family: var(--nick);
  }
  .hint,
  .note {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-soft);
  }
  .note {
    color: var(--warn);
  }
  .sure {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    color: var(--ink-soft);
  }
  .danger-solid {
    background: #c94a4a;
    color: white;
  }
  .danger-solid:hover:not(:disabled) {
    background: #d95a5a;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 13px;
  }
  .done {
    margin: 0;
    color: var(--live);
    font-size: 13px;
  }
</style>
