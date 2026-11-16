<script lang="ts">
  // The player in 3D, or the skin's flat picture where WebGL is missing.
  import { IdleAnimation, SkinViewer } from 'skinview3d'
  import { onMount } from 'svelte'
  import type { Skin } from './api'
  import { thumbnail } from './skin'

  let {
    skin,
    width = 280,
    height = 400,
    zoom = 0.94,
    name,
    still = false,
  }: {
    skin: Skin | undefined
    width?: number
    height?: number
    zoom?: number
    name?: string
    /** No turning with the mouse. */
    still?: boolean
  } = $props()

  let canvas: HTMLCanvasElement
  let viewer = $state.raw<SkinViewer | null>(null)
  let flat = $state(false)

  onMount(() => {
    try {
      // A still player is posed: turned toward the page's middle, looking at the viewer, one
      // foot forward, with a narrow lens so the head is not blown up.
      const v = new SkinViewer({ canvas, width, height, zoom, fov: still ? 20 : 38 })
      v.animation = new IdleAnimation()
      v.controls.enableZoom = false
      v.controls.enablePan = false
      v.controls.enableRotate = !still
      v.playerObject.rotation.y = still ? -0.5 : 0.5
      if (still) {
        const body = v.playerObject.skin
        body.head.rotation.set(0.06, 0.3, 0)
        body.rightLeg.rotation.set(0.14, 0, 0.03)
        body.leftLeg.rotation.set(-0.08, 0, -0.03)
      }
      viewer = v
    } catch {
      flat = true
    }
    // Nothing to draw for a window nobody sees.
    const pause = () => viewer && (viewer.renderPaused = document.hidden)
    document.addEventListener('visibilitychange', pause)
    return () => {
      document.removeEventListener('visibilitychange', pause)
      viewer?.dispose()
    }
  })

  // The window can be resized; the viewer keeps up.
  $effect(() => viewer?.setSize(width, height))

  $effect(() => {
    const s = skin
    if (!viewer) return
    if (s) void viewer.loadSkin(s.url, { model: s.model === 'slim' ? 'slim' : 'default' })
    else viewer.loadSkin(null)
  })
</script>

<div class="character" style="width:{width}px;height:{height}px">
  <canvas bind:this={canvas} class:hidden={flat || !skin} class:still></canvas>
  {#if flat && skin}
    {#await thumbnail(skin) then src}<img {src} alt="" />{/await}
  {/if}
  {#if !skin}
    <!-- No skin yet: a grey figure stands where the player will. -->
    <svg class="ghost" viewBox="0 0 16 32" shape-rendering="crispEdges" aria-hidden="true">
      <path d="M4 0h8v8H4zM4 8h8v12H4zM0 8h4v12H0zM12 8h4v12h-4zM4 20h4v12H4zM8 20h4v12H8z" fill="currentColor" />
    </svg>
  {/if}
  {#if name}<div class="tag">{name}</div>{/if}
  <div class="floor"></div>
</div>

<style>
  .character {
    position: relative;
    display: grid;
    place-items: center;
  }
  canvas {
    position: relative;
    z-index: 1;
    cursor: grab;
  }
  canvas:active {
    cursor: grabbing;
  }
  canvas.still {
    cursor: default;
  }
  .hidden {
    display: none;
  }
  img {
    height: 88%;
    image-rendering: pixelated;
    z-index: 1;
  }
  .ghost {
    position: absolute;
    height: 84%;
    color: rgba(255, 255, 255, 0.09);
  }
  /* The name over the head, as other players see it in the game. */
  .tag {
    position: absolute;
    z-index: 2;
    top: -2%;
    left: 50%;
    transform: translateX(-50%);
    padding: 1px 10px 3px;
    background: rgba(0, 0, 0, 0.42);
    color: #f2f2f2;
    font: 600 28px/1.3 var(--nick);
    white-space: nowrap;
    pointer-events: none;
  }
  .floor {
    position: absolute;
    bottom: 4%;
    left: 50%;
    width: 64%;
    height: 7%;
    transform: translateX(-50%);
    border-radius: 50%;
    background: radial-gradient(closest-side, rgba(0, 0, 0, 0.6), transparent);
  }
</style>
