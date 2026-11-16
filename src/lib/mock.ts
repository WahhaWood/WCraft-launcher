// Stand-in for the Rust side when the UI runs in a plain browser (vite dev). Not in release builds.
import type { Library, Settings, Shader, Skin, Status } from './api'

const handlers = new Map<string, ((p: any) => void)[]>()
const emit = (event: string, payload: unknown) => handlers.get(event)?.forEach((h) => h(payload))
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms))
const params = new URLSearchParams(location.search)
if (params.has('shot')) new Image().src = '/__slow'
// `?click` presses the main button, to look at the busy states.
if (params.has('click')) setTimeout(() => document.querySelector<HTMLElement>('.play')?.click(), 700)

/** A made-up skin: flat colours where the head, body, arms and legs are. */
function fakeSkin(hair: string, skin: string, shirt: string, legs: string): string {
  const c = document.createElement('canvas')
  c.width = c.height = 64
  const g = c.getContext('2d')!
  const fill = (color: string, x: number, y: number, w: number, h: number) => {
    g.fillStyle = color
    g.fillRect(x, y, w, h)
  }
  fill(hair, 0, 0, 32, 16)
  fill(skin, 8, 8, 8, 8)
  fill(hair, 8, 8, 8, 2)
  fill('#222', 10, 12, 1, 1)
  fill('#222', 13, 12, 1, 1)
  fill(shirt, 16, 16, 24, 16)
  fill(shirt, 40, 16, 16, 16)
  fill(skin, 44, 28, 12, 4)
  fill(legs, 0, 16, 16, 16)
  fill(legs, 16, 48, 16, 16)
  fill(shirt, 32, 48, 16, 16)
  fill(skin, 36, 60, 12, 4)
  return c.toDataURL()
}

// The real default skins the launcher bundles.
const bundled = import.meta.glob<string>('../../src-tauri/skins/*.png', { eager: true, query: '?url', import: 'default' })
const SLIM = ['alex', 'efe', 'makena', 'noor']

const library: Library = {
  active: 'mine',
  saved: [{ id: 'mine', name: 'Мой скин', model: 'classic', url: fakeSkin('#1d8f95', '#c9a183', '#15151c', '#101016') }],
  defaults: Object.entries(bundled).map(([path, url]) => {
    const id = path.split('/').pop()!.replace('.png', '')
    return { id: `default:${id}`, name: id[0].toUpperCase() + id.slice(1), model: SLIM.includes(id) ? 'slim' : 'classic', url }
  }) as Skin[],
}

// `?defaults` wears a default skin, so the skins page opens on that tab.
if (params.has('defaults')) library.active = 'default:steve'

const fresh = params.has('fresh')
const status: Status = {
  cli: fresh ? null : '0.2.58',
  account: fresh ? null : { name: 'WahhaWD', uuid: '19543b09702d4058817e59cda438362f', offline: true },
  accounts: fresh
    ? []
    : [
        { name: 'WahhaWD', uuid: '19543b09702d4058817e59cda438362f', offline: true },
        { name: 'Dara', uuid: '2', offline: true },
      ],
  owner: true,
  installed: !fresh,
  prepared: !fresh,
  instance: !fresh,
  pack: { server: 'play.wcraft.dev', horizon_version: 'nightly-2026-11-13-9f42064', instance: 'WCraft', auth: 'http://127.0.0.1:8095' },
  server: 'play.wcraft.dev',
  playing: false,
  version: '0.1.0',
  logged_in: fresh ? [] : ['WahhaWD'],
}
let settings: Settings = { server: '', while_playing: 'minimize', game_args: '', accent: 'violet' }

// A few of Horizon's registry shader packs, as shaders_list gives them. `?shaders` has two installed, one on.
const shaders: Shader[] = [
  {
    "id": "wisdom_horizon",
    "name": "Wisdom Shaders V3 (Horizon port)",
    "summary": "bobcao3's Wisdom Shaders V3 ported to Horizon's native shader API.",
    "description": "bobcao3's Wisdom Shaders V3 (Build 3300) ported to Horizon's shader API: its sun, ambient and torch colours, Oren-Nayar and GGX lighting, jittered soft shadows, atmosphere, 2D clouds, fog and AgX tonemap, translated from GLSL to WGSL. Adds bloom, FXAA, mob and chest relighting, held light sources and a horizon haze as Horizon post passes.\n\nUnofficial port by iLoong. A lighter version for more FPS ",
    "authors": [
      "bobcao3",
      "iLoong"
    ],
    "icon": "https://horizon-rust.github.io/horizon-mods/icons/wisdom_horizon.png",
    "screenshots": [
      "https://horizon-rust.github.io/horizon-mods/screenshots/wisdom_horizon/1.png",
      "https://horizon-rust.github.io/horizon-mods/screenshots/wisdom_horizon/2.png",
      "https://horizon-rust.github.io/horizon-mods/screenshots/wisdom_horizon/3.png"
    ],
    "categories": [
      "semi-realistic",
      "shadows",
      "high"
    ],
    "downloads": 83115,
    "version": "1.3.7",
    "size": 39196,
    "page": "https://playhorizon.dev/mods/wisdom_horizon",
    "license": "Apache-2.0",
    "local": false,
    "file": null,
    "installed_version": null,
    "enabled": false
  },
  {
    "id": "wisdom_horizon_lite",
    "name": "Wisdom Shaders V3 Lite (Horizon port)",
    "summary": "A lighter Wisdom Shaders V3 port for Horizon, for more FPS.",
    "description": "A lighter version of the Wisdom Shaders V3 port (`wisdom_horizon`) for more FPS: the same lighting, atmosphere, fog and AgX tonemap, with a smaller shadow map, no bloom or FXAA, and soft shadows, cloud shadows and water reflections off by default (each can be turned back on in the settings).\n\nUnofficial port of bobcao3's Wisdom Shaders V3 by tomasmendes01 (in-game name: iLoong).",
    "authors": [
      "bobcao3",
      "tomasmendes01"
    ],
    "icon": "https://horizon-rust.github.io/horizon-mods/icons/wisdom_horizon_lite.png",
    "screenshots": [
      "https://horizon-rust.github.io/horizon-mods/screenshots/wisdom_horizon_lite/1.png",
      "https://horizon-rust.github.io/horizon-mods/screenshots/wisdom_horizon_lite/2.png",
      "https://horizon-rust.github.io/horizon-mods/screenshots/wisdom_horizon_lite/3.png"
    ],
    "categories": [
      "semi-realistic",
      "atmosphere",
      "low"
    ],
    "downloads": 62983,
    "version": "1.3.7",
    "size": 37136,
    "page": "https://playhorizon.dev/mods/wisdom_horizon_lite",
    "license": "Apache-2.0",
    "local": false,
    "file": null,
    "installed_version": null,
    "enabled": false
  },
  {
    "id": "daybreak",
    "name": "Daybreak",
    "summary": "Sun shadows, reflective water, waving plants, coloured light, sun rays and bloom, built to cost as little GPU time as possible.",
    "description": "Daybreak is a shader pack in Horizon's own format. It runs inside Horizon's renderer, so Horizon's culling, batching and LOD stay in charge, and it adds as little GPU time as it can.\n\n- Sun and moon shadows with smooth edges\n- Warm sunlight, blue sky light in the shade, a dim blue night\n- Coloured block light: soul fire blue, redstone red, copper green, froglights\n- Light from torches in your hand",
    "authors": [
      "hildlabs"
    ],
    "icon": "https://horizon-rust.github.io/horizon-mods/icons/daybreak.png",
    "screenshots": [
      "https://horizon-rust.github.io/horizon-mods/screenshots/daybreak/1.jpg",
      "https://horizon-rust.github.io/horizon-mods/screenshots/daybreak/2.jpg",
      "https://horizon-rust.github.io/horizon-mods/screenshots/daybreak/3.jpg"
    ],
    "categories": [
      "semi-realistic",
      "shadows",
      "medium"
    ],
    "downloads": 41543,
    "version": "0.2.0",
    "size": 20496,
    "page": "https://playhorizon.dev/mods/daybreak",
    "license": "MIT",
    "local": false,
    "file": null,
    "installed_version": null,
    "enabled": false
  },
  {
    "id": "montarc",
    "name": "Montarc",
    "summary": "Bright, colorful shader pack: soft shadows, colored light, living water, wind, and own look for the Nether and the End.",
    "description": "Bright, colorful shader pack for Horizon. Horizon had no shaders on the registry, so I made one. Style is close to Complementary Reimagined, but the code is mine, written from scratch in WGSL.\n\n## What you get\n\n- **Soft shadows.** Sun and moon cast them. Water and stained glass tint the light that goes through.\n- **Sky and fog match.** Blue day, orange sunsets, dark blue night. Far terrain fades i",
    "authors": [
      "Muurrcc"
    ],
    "icon": "https://horizon-rust.github.io/horizon-mods/icons/montarc.png",
    "screenshots": [],
    "categories": [
      "semi-realistic",
      "colored-lighting",
      "shadows"
    ],
    "downloads": 34168,
    "version": "1.1.1",
    "size": 23426,
    "page": "https://playhorizon.dev/mods/montarc",
    "license": "MIT",
    "local": false,
    "file": null,
    "installed_version": null,
    "enabled": false
  },
  {
    "id": "radiant",
    "name": "Radiant",
    "summary": "Cinematic but playable: warm sun, soft shadows, light shafts, golden haze, sunset clouds, wet rainy ground and a filmic grade.",
    "description": "A cinematic shader pack for Horizon that is built to be played in, not just screenshotted.\n\n- **Light and shadow:** soft sun and moon shadows, light through leaves, coloured block light, flickering torches, ambient occlusion.\n- **Sky:** deep blue days, layered gold, pink and violet sunsets, a round sun, drifting clouds, twinkling stars and a faint Milky Way.\n- **Light shafts:** god rays through ca",
    "authors": [
      "Daniel"
    ],
    "icon": "https://horizon-rust.github.io/horizon-mods/icons/radiant.png",
    "screenshots": [],
    "categories": [
      "realistic",
      "atmosphere",
      "high"
    ],
    "downloads": 28695,
    "version": "1.0.0",
    "size": 42445,
    "page": "https://playhorizon.dev/mods/radiant",
    "license": "MIT",
    "local": false,
    "file": null,
    "installed_version": null,
    "enabled": false
  },
  {
    "id": "lumina",
    "name": "Lumina",
    "summary": "High performance with real-time shadows, physical water, roof overhang occlusion, volumetric 3D clouds, and sharp anti-aliasing",
    "description": "Native shaders designed for high frame rates and clean, balanced visuals without post-processing blur. All options are fully configurable directly in the shader settings menu.\n\n**Lighting & Overhang Shading**\n- Balanced daytime lighting with warm sun and clean azure shadows.\n- Directional sky dome occlusion: accurate contrast between ground, walls, and ceilings.\n- Non-linear ambient occlusion unde",
    "authors": [
      "cosmo-jar"
    ],
    "icon": "https://horizon-rust.github.io/horizon-mods/icons/lumina.png",
    "screenshots": [
      "https://horizon-rust.github.io/horizon-mods/screenshots/lumina/1.jpg",
      "https://horizon-rust.github.io/horizon-mods/screenshots/lumina/2.jpg",
      "https://horizon-rust.github.io/horizon-mods/screenshots/lumina/3.jpg",
      "https://horizon-rust.github.io/horizon-mods/screenshots/lumina/4.jpg"
    ],
    "categories": [
      "atmosphere",
      "shadows",
      "semi-realistic"
    ],
    "downloads": 21825,
    "version": "1.0.0",
    "size": 41770,
    "page": "https://playhorizon.dev/mods/lumina",
    "license": "GPL-3.0",
    "local": false,
    "file": null,
    "installed_version": null,
    "enabled": false
  }
]
if (params.has('shaders')) {
  Object.assign(shaders[0], { file: `${shaders[0].id}-${shaders[0].version}.hshader`, installed_version: shaders[0].version, enabled: true })
  Object.assign(shaders[2], { file: `${shaders[2].id}-1.0.0.hshader`, installed_version: '0.9.0' })
  shaders.push({ ...shaders[1], id: 'BSL_v8.zip', name: 'BSL_v8.zip', summary: 'shaders/ format', description: '', authors: [], icon: null, screenshots: [], categories: [], local: true, file: 'BSL_v8.zip', installed_version: null })
}
const shaderBy = (id: string) => shaders.find((s) => s.id === id)!

const commands: Record<string, (args: any) => unknown> = {
  status: () => ({ ...status }),
  // `wrong` as a password fails a login; `Taken` is a registered name.
  auth(a) {
    if (a.register && a.name.toLowerCase() === 'taken') throw 'Этот ник уже занят'
    if (!a.register && a.password === 'wrong') throw 'Неверный ник или пароль'
    if (!status.logged_in.includes(a.name)) status.logged_in.push(a.name)
    return commands.account_offline(a)
  },
  account_offline(a) {
    const acc = { name: a.name, uuid: Math.random().toString(16).slice(2, 10), offline: true }
    status.accounts = status.accounts.filter(x => x.name.toLowerCase() !== a.name.toLowerCase())
    status.accounts.push(acc)
    status.account = acc
    return acc
  },
  account_select(a) {
    const acc = status.accounts.find(x => x.name.toLowerCase() === a.name.toLowerCase())
    if (acc) status.account = acc
  },
  account_remove(a) {
    status.accounts = status.accounts.filter(x => x.name !== a.name)
    if (status.account?.name === a.name) status.account = status.accounts[0] || null
  },
  cancel: () => {},
  data_size: () => (fresh ? 0 : 1_712_000_000),
  wipe_data: () => location.reload(),
  async play() {
    for (const step of ['Устанавливаю Horizon', 'Готовлю ресурсы игры', 'Запускаю игру']) {
      emit('step', step)
      for (let i = 1; i <= 5; i++) {
        emit('log', `${step}: ${i * 20}%`)
        if (step !== 'Запускаю игру') emit('progress', { done: (step === 'Устанавливаю Horizon' ? i : 5 + i * 50) * 5e6, total: 1_466_000_000 })
        await wait(params.has('busy') ? 1e9 : 300)
      }
      if (step === 'Готовлю ресурсы игры') emit('progress', null)
    }
    Object.assign(status, { installed: true, prepared: true, instance: true, playing: true })
    setTimeout(() => {
      status.playing = false
      emit('game-exit', null)
    }, 5000)
  },
  server_status: () => {
    if (params.has('offline')) throw 'Connection refused'
    return { online: 3, max: 20, version: 'Horizon 26.3', ping_ms: 24, players: ['WahhaWD', 'Steve', 'Alex'] }
  },
  settings_get: () => settings,
  settings_set: (a) => void (settings = a.settings),
  skins_list: () => structuredClone(library),
  skins_add(a) {
    const url = URL.createObjectURL(new Blob([new Uint8Array(a.png)], { type: 'image/png' }))
    const id = Math.random().toString(16).slice(2, 10)
    library.saved.unshift({ id, name: a.name, model: a.model, url })
    library.active = id
    return id
  },
  skins_catalog: () => ({
    pages: 1,
    items: library.defaults.map((d, i) => ({ id: i.toString(16).padStart(16, '0'), title: d.name, model: d.model, preview: d.url, wearers: 100 })),
  }),
  skins_catalog_install: () => 'mock',
  skins_catalog_png: (a) => library.defaults[parseInt(a.id, 16) % library.defaults.length].url,
  skins_remove(a) {
    library.saved = library.saved.filter((s) => s.id !== a.id)
    if (library.active === a.id) library.active = null
  },
  skins_select: (a) => void (library.active = a.id),
  skins_set_model: (a) => void (library.saved.find((s) => s.id === a.id)!.model = a.model),
  skins_players: () => ({ Alex: { ...library.defaults[1], name: 'Alex' } }),
  update_check: () => (params.has('update') ? { version: '0.1.1', size: 37_600_000 } : null),
  async update_install() {
    for (let p = 0; p <= 100; p += 5) {
      emit('update-progress', p)
      await wait(120)
    }
    throw 'Сервер обновлений недоступен'
  },
  shaders_list: () => structuredClone(shaders),
  async shaders_install(a) {
    await wait(1200)
    const s = shaderBy(a.id)
    shaders.forEach((x) => (x.enabled = false))
    Object.assign(s, { file: `${s.id}-${s.version}.hshader`, installed_version: s.version, enabled: true })
  },
  shaders_update: (a) => void (shaderBy(a.id).installed_version = shaderBy(a.id).version),
  shaders_set: (a) => shaders.forEach((x) => (x.enabled = !!a.file && x.file === a.file)),
  shaders_remove(a) {
    const s = shaderBy(a.id)
    if (s.local) shaders.splice(shaders.indexOf(s), 1)
    else Object.assign(s, { file: null, installed_version: null, enabled: false })
  },
  shaders_add(a) {
    if (!/\.(zip|hshader)$/i.test(a.name)) throw 'Шейдеры — это файлы .zip или .hshader'
    shaders.push({ ...shaders[0], id: a.name, name: a.name, summary: 'shaders/ format', icon: null, screenshots: [], categories: [], local: true, file: a.name, enabled: false })
  },
  shaders_open_dir: () => {},
  account_rename: (a) => {
    if (a.password === 'wrong') throw 'Неверный пароль'
    return a.newName
  },
  account_password: (a) => {
    if (a.current === 'wrong') throw 'Неверный пароль'
  },
  account_logout_others: () => 1,
  account_delete: (a) => {
    if (a.password === 'wrong') throw 'Неверный пароль'
  },
  account_info: () => [Math.floor(Date.now() / 1000) - 86400 * 9, null],
  modrinth_search: (a) => {
    const base = shaders[0]
    return ['Complementary Reimagined', 'BSL Shaders', 'Rethinking Voxels', 'MakeUp - Ultra Fast', 'Photon']
      .filter((n) => n.toLowerCase().includes(a.query.toLowerCase()))
      .map((n, i) => ({ ...base, id: n.toLowerCase().replace(/\W+/g, '-'), name: n, summary: 'Шейдер с Modrinth', description: 'Шейдер с Modrinth', icon: null, screenshots: [], categories: ['iris', 'optifine'], downloads: 5e6 / (i + 1), file: null, installed_version: null, enabled: false, local: false, license: 'LicenseRef-Custom' }))
  },
  resources_list: () => [
    { ...shaders[0], id: 'Faithful.zip', name: 'Faithful 32x', summary: 'Ресурспак', description: '', authors: [], icon: null, screenshots: [], categories: [], local: true, file: 'Faithful.zip', installed_version: null, enabled: true },
  ],
  resources_set: () => {},
  resources_remove: () => {},
  resources_add: () => {},
  resources_open_dir: () => {},
  modrinth_gallery: () => [],
  modrinth_install: () => new Promise((r) => setTimeout(r, 800)),
  open_link: () => {},
  open_data_dir: () => {},
}

export default {
  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    await wait(60)
    return (await commands[cmd](args)) as T
  },
  on(event: string, handler: (p: any) => void) {
    handlers.set(event, [...(handlers.get(event) ?? []), handler])
  },
}
