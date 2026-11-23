import { api, on, type Library, type ServerStatus, type Settings, type Skin, type Status, type Update } from './api'

export type Page = 'play' | 'skins' | 'shaders' | 'resources' | 'mods' | 'settings' | 'log'

export const app = $state({
  page: 'play' as Page,
  status: null as Status | null,
  /** A task of ours is running: setting up before the game starts, or compiling the game with own mods. */
  busy: null as null | 'setup' | 'build',
  /** When the running task started (ms), for the build's timer. */
  since: 0,
  step: '',
  lastLine: '',
  /** How far setup is in bytes, and the seconds it should still take (null until the pace is known). */
  progress: null as null | { done: number; total: number; eta: number | null; speed: number | null },
  log: [] as string[],
  error: '',
  playing: false,
  /** null: not asked yet; 'offline': no answer. */
  server: null as ServerStatus | 'offline' | null,
  settings: { server: '', connection_route: 'pack', while_playing: 'minimize', game_args: '', accent: 'violet', experimental_mods: false, storage: '' } as Settings,
  skins: null as Library | null,
  /** Skins the server's players published, by name. */
  players: {} as Record<string, Skin>,
  update: null as Update | null,
  /** Percent downloaded while the launcher updates itself; null when not updating. */
  updating: null as number | null,
})

export async function refresh() {
  app.status = await api.status()
  app.playing = app.status.playing
}

let pingRequest = 0

export async function pingServer() {
  const request = ++pingRequest
  let server: ServerStatus
  try {
    server = await api.serverStatus()
  } catch {
    if (request === pingRequest) app.server = 'offline'
    return
  }
  if (request !== pingRequest) return
  app.server = server
  // Faces for the players on the server; the old ones stay if the accounts service doesn't answer.
  if (server.players.length) {
    const players = await api.skinsPlayers().catch(() => app.players)
    if (request === pingRequest) app.players = players
  }
}

export async function loadSkins() {
  app.skins = await api.skinsList()
}

export function activeSkin() {
  const lib = app.skins
  return lib ? [...lib.saved, ...lib.defaults].find((s) => s.id === lib.active) : undefined
}

export async function saveSettings() {
  await api.settingsSet($state.snapshot(app.settings))
  // The server's address may be what changed.
  await refresh()
  void pingServer()
}

/** Recent progress readings: the pace over them gives the time left. */
let samples: { t: number; done: number }[] = []

function onProgress(p: { done: number; total: number } | null) {
  if (!p || !app.busy) {
    app.progress = null
    samples = []
    return
  }
  const t = Date.now()
  samples.push({ t, done: p.done })
  // The last half minute: steady enough, yet it follows the pace from one part of setup to the next.
  while (samples.length > 2 && t - samples[0].t > 30_000) samples.shift()
  const first = samples[0]
  const secs = (t - first.t) / 1000
  const rate = (p.done - first.done) / secs
  // The speed shown follows the last few seconds, so it reacts when the connection changes.
  const recent = samples.find((s) => t - s.t <= 5_000) ?? first
  const recentSecs = (t - recent.t) / 1000
  const speed = recentSecs >= 1 ? Math.max(0, (p.done - recent.done) / recentSecs) : null
  app.progress = { ...p, eta: secs >= 5 && rate > 0 ? Math.max(0, p.total - p.done) / rate : null, speed }
}

async function task(kind: 'setup' | 'build', step: string, run: () => Promise<void>) {
  if (app.busy) return
  app.error = ''
  app.busy = kind
  app.since = Date.now()
  app.step = step
  app.lastLine = ''
  onProgress(null)
  try {
    await run()
  } catch (e) {
    if (e !== 'Отменено') app.error = String(e)
  } finally {
    app.busy = null
    onProgress(null)
    await refresh()
  }
}

/** Plays on the newest launcher only: with an update out, the Play button turns into Update instead. */
export const play = () =>
  task('setup', 'Проверяю обновления', async () => {
    // Without an answer the server decides: it refuses a launcher older than its newest release.
    app.update = await api.updateCheck().catch(() => app.update)
    if (app.update) return
    app.step = 'Проверяю файлы'
    await api.play()
    app.playing = true
  })

/** Compiles the game with WCraft's mods and the player's own. Its error stays in `app.error`. */
export const buildMods = () =>
  task('build', 'Готовлю сборку', async () => {
    await api.modsBuild()
  })

export const authenticate = (register: boolean, name: string, password: string) =>
  task('setup', register ? 'Регистрирую аккаунт' : 'Вхожу в аккаунт', async () => {
    await api.auth(register, name, password)
  })

export async function selectAccount(name: string) {
  try {
    app.error = ''
    await api.accountSelect(name)
    await refresh()
  } catch (e) {
    app.error = String(e)
  }
}

export async function removeAccount(name: string, offline: boolean) {
  try {
    app.error = ''
    await api.accountRemove(name, offline)
    await refresh()
  } catch (e) {
    app.error = String(e)
  }
}

export async function installUpdate() {
  if (app.updating !== null || app.playing || app.busy) return
  app.error = ''
  app.updating = 0
  try {
    await api.updateInstall()
  } catch (e) {
    app.error = String(e)
  } finally {
    app.updating = null
  }
}

export async function init() {
  on<string>('step', (s) => (app.step = s))
  on('progress', onProgress)
  on<number>('update-progress', (p) => (app.updating = p))
  on<string>('log', (line) => {
    app.lastLine = line
    app.log.push(line)
    if (app.log.length > 600) app.log.splice(0, 100)
  })
  on<string | null>('game-exit', (error) => {
    app.playing = false
    if (error) app.error = error
    void refresh()
  })
  app.settings = await api.settingsGet()
  await Promise.all([refresh(), loadSkins()])
  void pingServer()
  // No answer about updates is no update: the launcher works as it is.
  api.updateCheck().then((u) => (app.update = u), () => {})
  // The server's state only matters while someone is looking.
  setInterval(() => !document.hidden && void pingServer(), 30_000)
  window.addEventListener('focus', () => !app.busy && void refresh())
}
