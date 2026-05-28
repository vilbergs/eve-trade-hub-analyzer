<script lang="ts">
  import { fetchPilots } from '../lib/api';
  import type { PilotEntry } from '../lib/api';

  let { channel }: { channel: string } = $props();

  let pilots: PilotEntry[] = $state([]);
  let top = 50;
  let sortKey: keyof PilotEntry = $state('sightings');
  let sortAsc: boolean = $state(false);

  $effect(() => {
    if (channel) {
      fetchPilots(channel, top).then(d => pilots = d.pilots);
    }
  });

  let sorted = $derived(
    [...pilots].sort((a, b) => {
      const av = a[sortKey];
      const bv = b[sortKey];
      const cmp = typeof av === 'string' ? av.localeCompare(bv as string) : (av as number) - (bv as number);
      return sortAsc ? cmp : -cmp;
    })
  );

  function toggleSort(key: keyof PilotEntry) {
    if (sortKey === key) {
      sortAsc = !sortAsc;
    } else {
      sortKey = key;
      sortAsc = key === 'name';
    }
  }

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString('en-GB', { day: '2-digit', month: 'short', year: '2-digit' });
  }
</script>

<div>
  <h2 class="section-title">Pilot Rap Sheet</h2>
  <p class="section-subtitle">Top {top} most-seen hostile pilots</p>

  <table class="data-table">
    <thead>
      <tr>
        <th class:is-sorted={sortKey === 'name'} onclick={() => toggleSort('name')}>Pilot</th>
        <th class="num" class:is-sorted={sortKey === 'sightings'} onclick={() => toggleSort('sightings')}>Sightings</th>
        <th class="num" class:is-sorted={sortKey === 'distinct_systems'} onclick={() => toggleSort('distinct_systems')}>Systems</th>
        <th class:is-sorted={sortKey === 'last_seen'} onclick={() => toggleSort('last_seen')}>Last Seen</th>
      </tr>
    </thead>
    <tbody>
      {#each sorted as p}
        <tr>
          <td class="name">{p.name}</td>
          <td class="num">{p.sightings}</td>
          <td class="num">{p.distinct_systems}</td>
          <td>{formatDate(p.last_seen)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
