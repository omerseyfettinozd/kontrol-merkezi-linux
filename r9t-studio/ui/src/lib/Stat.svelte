<script>
  // Tek ölçüm: değer yoksa "—", eskiyse (age_ms > stale) soluk gösterilir; asla 0 uydurulmaz.
  import { fmt } from './state.svelte.js';

  let { label, field = null, value = undefined, unit = '', digits = 0, stale = 10000, big = false, hint = '' } = $props();

  const v = $derived(value !== undefined ? value : field?.value ?? null);
  const old = $derived(field && field.age_ms != null && field.age_ms > stale);
  const text = $derived(typeof v === 'number' ? fmt(v, digits) : v == null ? '—' : String(v));
</script>

<div class="stat" title={hint || (field?.age_ms != null ? `${(field.age_ms / 1000).toFixed(1)} sn önce` : field ? 'veri yok' : '')}>
  <span class="label">{label}</span>
  <span class={big ? 'value-xl' : 'value-lg'} class:stale={old || v == null}>{text}{#if v != null && unit}<small>{unit}</small>{/if}</span>
</div>

<style>
  .stat { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  small { font-size: 12px; font-weight: 500; color: var(--muted); margin-left: 3px; }
</style>
