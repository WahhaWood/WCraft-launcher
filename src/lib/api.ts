import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type Account = { name: string; uuid: string; offline: boolean }

export type Status = {
  cli: string | null
  account: Account | null
  accounts: Account[]
  owner: boolean
  installed: boolean
  prepared: boolean
  instance: boolean
  pack: { server: string; horizon_version: string; instance: string; auth: string }
  server: string
  playing: boolean
  version: string
  /** Names logged in to their WCraft account on this computer. */
  logged_in: string[]
}
export type ServerStatus = { online: number; max: number; version: string; ping_ms: number; players: string[] }
export type Settings = { server: string; while_playing: 'stay' | 'minimize' | 'hide'; game_args: string; accent: string }
export type Skin = { id: string; name: string; model: 'classic' | 'slim'; url: string }
export type Update = { version: string; size: number }
export type CatalogSkin = { id: string; title: string; model: 'classic' | 'slim'; preview: string; wearers: number }
export type CatalogPage = { items: CatalogSkin[]; pages: number }
export type Library = { active: string | null; saved: Skin[]; defaults: Skin[] }
export type Shader = {
  /** The registry's id; for a pack added from a file, its file name. */
  id: string
  name: string
  summary: string
  description: string
  authors: string[]
  icon: string | null
  screenshots: string[]
  categories: string[]
  downloads: number
  version: string
  size: number
  page: string
  license: string
  /** Added from a file, not from Horizon's registry. */
  local: boolean
  /** The Modrinth project slug, when the file came from Modrinth. */
  modrinth?: string | null
  /** Its file in shaderpacks/ when installed. */
  file: string | null
  installed_version: string | null
  enabled: boolean
}

export const isTauri = '__TAURI_INTERNALS__' in window

// In a plain browser (vite dev) a mock stands in for the Rust side, to work on the UI.
const mock = import.meta.env.DEV && !isTauri ? (await import('./mock')).default : null

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return mock ? mock.invoke(cmd, args) : invoke<T>(cmd, args)
}

export function on<T>(event: string, handler: (payload: T) => void) {
  if (mock) mock.on(event, handler)
  else void listen<T>(event, (e) => handler(e.payload))
}

export const api = {
  status: () => call<Status>('status'),
  auth: (register: boolean, name: string, password: string) => call<Account>('auth', { register, name, password }),
  accountSelect: (name: string) => call<void>('account_select', { name }),
  /** A new name for a logged-in account; its saved data on the server goes with it. Gives the name as registered. */
  accountRename: (name: string, password: string, newName: string) => call<string>('account_rename', { name, password, newName }),
  accountPassword: (name: string, current: string, newPassword: string) => call<void>('account_password', { name, current, new: newPassword }),
  accountLogoutOthers: (name: string) => call<number>('account_logout_others', { name }),
  accountDelete: (name: string, password: string) => call<void>('account_delete', { name, password }),
  /** When the account was made and when its name last changed (Unix seconds). */
  accountInfo: (name: string) => call<[number, number | null]>('account_info', { name }),
  accountRemove: (name: string, offline: boolean) => call<void>('account_remove', { name, offline }),
  cancel: () => call<void>('cancel'),
  play: () => call<void>('play'),
  serverStatus: () => call<ServerStatus>('server_status'),
  /** A newer launcher on the server, if this build can update itself. */
  updateCheck: () => call<Update | null>('update_check'),
  /** Downloads it in place of this one and restarts; resolves only on failure. */
  updateInstall: () => call<void>('update_install'),
  settingsGet: () => call<Settings>('settings_get'),
  settingsSet: (settings: Settings) => call<void>('settings_set', { settings }),
  skinsList: () => call<Library>('skins_list'),
  skinsAdd: (name: string, model: string, png: number[]) => call<string>('skins_add', { name, model, png }),
  /** A page of the online skin catalog (millida.net). */
  skinsCatalog: (query: string, page: number, tag: string, sort: string) => call<CatalogPage>('skins_catalog', { query, page, tag, sort }),
  skinsCatalogPng: (id: string) => call<string>('skins_catalog_png', { id }),
  skinsCatalogInstall: (id: string, title: string, model: string) => call<string>('skins_catalog_install', { id, title, model }),
  skinsRemove: (id: string) => call<void>('skins_remove', { id }),
  skinsSelect: (id: string) => call<void>('skins_select', { id }),
  skinsSetModel: (id: string, model: string) => call<void>('skins_set_model', { id, model }),
  /** The server's players' published skins by name. */
  skinsPlayers: () => call<Record<string, Skin>>('skins_players'),
  shadersList: () => call<Shader[]>('shaders_list'),
  /** Downloads it from Horizon's registry and turns it on. */
  shadersInstall: (id: string) => call<void>('shaders_install', { id }),
  shadersUpdate: (id: string) => call<void>('shaders_update', { id }),
  /** Turns this pack file on, or with null plays without shaders. */
  shadersSet: (file: string | null) => call<void>('shaders_set', { file }),
  shadersRemove: (id: string) => call<void>('shaders_remove', { id }),
  /** A .zip or .hshader file: its bytes go as the raw body. */
  shadersAdd: async (file: File) => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    const headers = { 'x-name': encodeURIComponent(file.name) }
    return mock ? mock.invoke<void>('shaders_add', { name: file.name }) : invoke<void>('shaders_add', bytes, { headers })
  },
  /** A page of Modrinth's shaders; installing downloads, checks and turns the newest on. */
  modrinthSearch: (kind: 'shader' | 'resource', query: string, offset: number) => call<Shader[]>('modrinth_search', { kind, query, offset }),
  /** A project's screenshots at full size (search gives thumbnails). */
  modrinthGallery: (slug: string) => call<string[]>('modrinth_gallery', { slug }),
  modrinthInstall: (kind: 'shader' | 'resource', slug: string) => call<void>('modrinth_install', { kind, slug }),
  /** The instance's resource packs (a Shader with only its file, title and whether it is on). */
  resourcesList: () => call<Shader[]>('resources_list'),
  resourcesSet: (file: string, on: boolean) => call<void>('resources_set', { file, on }),
  resourcesRemove: (file: string) => call<void>('resources_remove', { file }),
  resourcesAdd: async (file: File) => {
    const bytes = new Uint8Array(await file.arrayBuffer())
    const headers = { 'x-name': encodeURIComponent(file.name) }
    return mock ? mock.invoke<void>('resources_add', { name: file.name }) : invoke<void>('resources_add', bytes, { headers })
  },
  resourcesOpenDir: () => call<void>('resources_open_dir'),
  shadersOpenDir: () => call<void>('shaders_open_dir'),
  openLink: (url: string) => call<void>('open_link', { url }),
  openDataDir: () => call<void>('open_data_dir'),
  dataSize: () => call<number>('data_size'),
  wipeData: () => call<void>('wipe_data'),
}
