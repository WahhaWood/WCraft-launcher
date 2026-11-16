<script lang="ts">
  const paths = {
    play: 'M7 4.5v15l12-7.5z',
    shirt: 'M9 3 3.5 6l1.8 4.2L8 9v11h8V9l2.7 1.2L20.5 6 15 3a3 3 0 0 1-6 0Z',
    sliders: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 4.5v5M9 14.5v5',
    terminal: 'M5 7l5 5-5 5M13 17h6',
    plus: 'M12 5v14M5 12h14',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    x: 'M6 6l12 12M18 6 6 18',
    minus: 'M5 12h14',
    maximize: 'M6 6h12v12H6z',
    restore: 'M8.5 9.5h10v10h-10zM5.5 14.5v-9h9',
    copy: 'M9 9h10v11H9zM5 15V4h10',
    trash: 'M4 7h16M9 7V4h6v3M6.5 7l1 13h9l1-13M10 11v5M14 11v5',
    folder: 'M3.5 6.5h6l2 2.5h9v10h-17z',
    link: 'M14 5h5v5M19 5l-8 8M17 14v5H5V7h5',
    chevron: 'M6 9l6 6 6-6',
    rotate: 'M4 12h16M8 8l-4 4 4 4M16 8l4 4-4 4',
    download: 'M12 4v11M7 10.5l5 5 5-5M5 20h14',
    sparkle: 'M12 3.5l1.9 5.1 5.1 1.9-5.1 1.9-1.9 5.1-1.9-5.1L5 10.5l5.1-1.9zM18.5 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z',
    search: 'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM16 16l4 4',
    user: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8ZM4.5 20a7.5 7.5 0 0 1 15 0',
  }
  let { name, size = 16 }: { name: keyof typeof paths; size?: number } = $props()
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={name === 'play' ? 'currentColor' : 'none'}
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name]} />
</svg>
