<script lang="ts">
  import Icon from '../lib/Icon.svelte'
  import { app } from '../lib/state.svelte'

  let box: HTMLElement
  let copied = $state(false)

  // Follow the newest line.
  $effect(() => {
    void app.log.length
    box?.scrollTo(0, box.scrollHeight)
  })

  async function copy() {
    await navigator.clipboard.writeText(app.log.join('\n')).catch(() => {})
    copied = true
    setTimeout(() => (copied = false), 1500)
  }
</script>

<div class="head">
  <div>
    <h1>Журнал</h1>
    <p>Что лаунчер делал в этом запуске.</p>
  </div>
  <button class="btn" disabled={!app.log.length} onclick={copy}><Icon name={copied ? 'check' : 'copy'} /> Скопировать</button>
</div>
<pre bind:this={box}>{app.log.join('\n') || 'Пока пусто.'}</pre>

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 16px;
  }
  h1 {
    font: 600 24px/1.2 var(--font);
    letter-spacing: -0.02em;
  }
  p {
    color: var(--muted);
    font-size: 13px;
  }
  pre {
    flex: 1;
    min-height: 0;
    margin: 16px 0 0;
    padding: 12px 14px;
    overflow: auto;
    border-radius: 12px;
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid var(--rule);
    color: var(--ink-soft);
    font: 12px/1.6 var(--mono);
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
    -webkit-user-select: text;
  }
</style>
