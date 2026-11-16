<script lang="ts">
  // The WTube icon (projects/wtube ic_launcher_*.xml), same as src-tauri/icons/source.svg.
  let { size = 26 }: { size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="16 16 76 76" aria-hidden="true">
  <defs>
    <clipPath id="logo-tile"><rect x="16" y="16" width="76" height="76" rx="17" /></clipPath>
    <linearGradient id="logo-bg" x1="0" y1="0" x2="108" y2="108" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#2A1766" />
      <stop offset="1" stop-color="#081A38" />
    </linearGradient>
    <radialGradient id="logo-cyan" cx="38" cy="40" r="24" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#3FD0FF" />
      <stop offset="1" stop-color="#3FD0FF" stop-opacity="0" />
    </radialGradient>
    <radialGradient id="logo-violet" cx="72" cy="70" r="26" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#C05CFF" />
      <stop offset="1" stop-color="#C05CFF" stop-opacity="0" />
    </radialGradient>
    <linearGradient id="logo-rim" x1="34" y1="34" x2="74" y2="74" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#fff" stop-opacity=".8" />
      <stop offset=".5" stop-color="#fff" stop-opacity=".1" />
      <stop offset="1" stop-color="#fff" stop-opacity=".45" />
    </linearGradient>
  </defs>
  <g clip-path="url(#logo-tile)">
    <rect width="108" height="108" fill="url(#logo-bg)" />
    <rect width="108" height="108" fill="url(#logo-cyan)" />
    <rect width="108" height="108" fill="url(#logo-violet)" />
  </g>
  <path
    d="M47 33h14a14 14 0 0 1 14 14v14a14 14 0 0 1-14 14h-14a14 14 0 0 1-14-14v-14a14 14 0 0 1 14-14z"
    fill="#fff"
    fill-opacity=".16"
    stroke="url(#logo-rim)"
    stroke-width="1.3"
  />
  <path
    d="M41.82 45.59 47.91 62.41 54 49.07 60.09 62.41 66.18 45.59"
    fill="none"
    stroke="#fff"
    stroke-linecap="round"
    stroke-linejoin="round"
    stroke-width="5.22"
  />
</svg>
