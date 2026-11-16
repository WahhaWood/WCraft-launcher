// Renders skins: the still pictures on the cards (one shared WebGL viewer, drawn one at a time
// and cached), and a flat 2D picture where WebGL is missing.
import { SkinViewer } from 'skinview3d'
import type { Skin } from './api'

const cache = new Map<string, Promise<string>>()
let viewer: SkinViewer | null | undefined
let queue: Promise<unknown> = Promise.resolve()

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => resolve(img)
    img.onerror = reject
    img.src = url
  })
}

/** The front of the player, 16x32 skin pixels, drawn flat. */
async function flat(skin: Skin): Promise<string> {
  const img = await loadImage(skin.url)
  const c = document.createElement('canvas')
  c.width = 16
  c.height = 32
  const g = c.getContext('2d')!
  const old = img.height === 32
  const part = (sx: number, sy: number, w: number, h: number, dx: number, dy: number) =>
    g.drawImage(img, sx, sy, w, h, dx, dy, w, h)
  part(8, 8, 8, 8, 4, 0)
  part(20, 20, 8, 12, 4, 8)
  part(44, 20, 4, 12, 0, 8)
  part(4, 20, 4, 12, 4, 20)
  if (old) {
    part(44, 20, 4, 12, 12, 8)
    part(4, 20, 4, 12, 8, 20)
  } else {
    part(36, 52, 4, 12, 12, 8)
    part(20, 52, 4, 12, 8, 20)
    part(40, 8, 8, 8, 4, 0)
  }
  return c.toDataURL()
}

async function render(skin: Skin): Promise<string> {
  if (viewer === undefined) {
    try {
      viewer = new SkinViewer({
        canvas: document.createElement('canvas'),
        width: 300,
        height: 380,
        fov: 28,
        renderPaused: true,
        preserveDrawingBuffer: true,
      })
      // Head to hips, turned a little, as a portrait.
      viewer.playerObject.rotation.y = 0.42
      viewer.camera.position.set(0, 11, 49)
      viewer.controls.target.set(0, 5.5, 0)
      viewer.controls.update()
    } catch {
      viewer = null
    }
  }
  if (!viewer) return flat(skin)
  await viewer.loadSkin(skin.url, { model: skin.model === 'slim' ? 'slim' : 'default' })
  viewer.render()
  return viewer.canvas.toDataURL()
}

export function thumbnail(skin: Skin): Promise<string> {
  const key = `${skin.id}:${skin.model}`
  let job = cache.get(key)
  if (!job) {
    job = queue.then(() => render(skin)).catch(() => flat(skin))
    queue = job.catch(() => {})
    cache.set(key, job)
  }
  return job
}

/** Checks a dropped file is a skin and tells its arms: slim skins leave a column of the arm empty. */
export async function inspect(file: File): Promise<{ model: 'classic' | 'slim'; png: number[] }> {
  if (file.size > 1 << 20) throw 'Файл слишком большой для скина'
  const bytes = new Uint8Array(await file.arrayBuffer())
  const url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))
  try {
    const img = await loadImage(url).catch(() => {
      throw 'Это не картинка PNG'
    })
    if (img.width !== 64 || (img.height !== 64 && img.height !== 32))
      throw `Скин должен быть 64×64 или 64×32, а этот ${img.width}×${img.height}`
    const c = document.createElement('canvas')
    c.width = 64
    c.height = img.height
    const g = c.getContext('2d')!
    g.drawImage(img, 0, 0)
    const slim = img.height === 64 && g.getImageData(54, 20, 1, 1).data[3] === 0
    return { model: slim ? 'slim' : 'classic', png: Array.from(bytes) }
  } finally {
    URL.revokeObjectURL(url)
  }
}
