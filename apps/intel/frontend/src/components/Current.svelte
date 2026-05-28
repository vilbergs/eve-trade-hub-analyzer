<script lang="ts">
  import { fetchCurrent } from '../lib/api';
  import type { Threat } from '../lib/api';
  import { onDestroy } from 'svelte';

  let { channel }: { channel: string } = $props();

  let threats: Threat[] = $state([]);
  let lastRefresh: string = $state('');
  let interval: ReturnType<typeof setInterval> | null = null;

  function refresh() {
    fetchCurrent(channel).then(d => {
      threats = d.threats;
      lastRefresh = new Date().toLocaleTimeString();
    });
  }

  $effect(() => {
    if (channel) {
      refresh();
      if (interval) clearInterval(interval);
      interval = setInterval(refresh, 15_000);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  });

  function ago(iso: string): string {
    const d = new Date(iso);
    const m = Math.floor((Date.now() - d.getTime()) / 60000);
    if (m < 1) return 'just now';
    if (m < 60) return `${m}m ago`;
    if (m < 1440) return `${Math.floor(m/60)}h ${m%60}m ago`;
    return `${Math.floor(m/1440)}d ago`;
  }
</script>

<div>
  <h2 class="section-title">Current Threats</h2>
  <p class="section-subtitle">
    Systems with recent hostile activity · auto-refreshes every 15s
    {#if lastRefresh}
      · last update: {lastRefresh}
    {/if}
  </p>

  {#if threats.length === 0}
    <div class="threats-empty">
      <div class="threats-empty-icon">✅</div>
      <div class="threats-empty-text">All clear — no active threats</div>
    </div>
  {:else}
    <table class="data-table">
      <thead>
        <tr>
          <th>Channel</th>
          <th>System</th>
          <th>Started</th>
          <th>Last Activity</th>
        </tr>
      </thead>
      <tbody>
        {#each threats as t}
          <tr>
            <td>{t.channel}</td>
            <td class="name">{t.system}</td>
            <td>{ago(t.started_at)}</td>
            <td>{ago(t.ended_at)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
