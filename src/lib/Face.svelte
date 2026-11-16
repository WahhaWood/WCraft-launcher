<script lang="ts">
  // The face of a skin: its 8x8 front of the head and the hat layer over it, straight from the PNG.
  let { url, size = 32 }: { url: string; size?: number } = $props()
  const k = $derived(size / 8)
</script>

<span
  class="face"
  style="width:{size}px;height:{size}px;background-image:url({url}),url({url});background-size:{64 * k}px auto;background-position:{-40 *
    k}px {-8 * k}px,{-8 * k}px {-8 * k}px"
></span>

<style>
  .face {
    display: block;
    flex: none;
    border-radius: 6px;
    image-rendering: pixelated;
    background-repeat: no-repeat;
  }
</style>
