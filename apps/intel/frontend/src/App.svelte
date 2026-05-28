<script lang="ts">
    import { fetchChannels, fetchStats } from "./lib/api";
    import type { Channel, Stats } from "./lib/api";
    import Heatmap from "./components/Heatmap.svelte";
    import Systems from "./components/Systems.svelte";
    import Pilots from "./components/Pilots.svelte";
    import Safety from "./components/Safety.svelte";
    import Current from "./components/Current.svelte";

    type View = "heatmap" | "systems" | "pilots" | "safety" | "current";

    let view: View = $state("heatmap");
    let channels: Channel[] = $state([]);
    let channel: string = $state("");
    let weeks: number = $state(0);
    let stats: Stats | null = $state(null);

    const views: { id: View; label: string }[] = [
        { id: "heatmap", label: "Threat Heatmap" },
        { id: "safety", label: "System Safety" },
        { id: "systems", label: "Hotspots" },
        { id: "pilots", label: "Pilots" },
        { id: "current", label: "Current" },
    ];

    $effect(() => {
        fetchChannels().then((chs) => {
            channels = chs;
            if (!channel && chs.length > 0) {
                channel = chs[0].name;
            }
        });
    });

    $effect(() => {
        if (channel) {
            fetchStats(channel, weeks).then((s) => (stats = s));
        }
    });
</script>

<div class="intel-app">
    <header class="intel-header">
        <div class="intel-brand">
            <span class="intel-brand-name">Intel</span>
            <span class="intel-brand-sub">threat analyzer</span>
        </div>
        <nav class="intel-nav">
            {#each views as v}
                <button
                    class="intel-nav-btn"
                    class:is-active={view === v.id}
                    onclick={() => (view = v.id)}>{v.label}</button
                >
            {/each}
        </nav>
        <div class="intel-controls">
            <select class="intel-select" bind:value={channel}>
                {#each channels as ch}
                    <option value={ch.name}>{ch.name}</option>
                {/each}
            </select>
            <select class="intel-select" bind:value={weeks}>
                <option value={0}>All time</option>
                <option value={4}>4 weeks</option>
                <option value={8}>8 weeks</option>
                <option value={12}>12 weeks</option>
                <option value={26}>26 weeks</option>
                <option value={52}>52 weeks</option>
            </select>
        </div>
    </header>

    {#if stats}
        <div class="intel-stats">
            <div class="intel-stat">
                <span class="intel-stat-label">Sightings</span>
                <span class="intel-stat-value">{stats.total_sightings}</span>
            </div>
            <div class="intel-stat">
                <span class="intel-stat-label">Systems Hit</span>
                <span class="intel-stat-value">{stats.total_systems_hit}</span>
            </div>
            <div class="intel-stat">
                <span class="intel-stat-label">Dirty Hours</span>
                <span class="intel-stat-value"
                    >{stats.total_dirty_hours.toFixed(1)}</span
                >
            </div>
            <div class="intel-stat">
                <span class="intel-stat-label">Observed</span>
                <span class="intel-stat-value"
                    >{stats.observation_hours.toFixed(1)}h</span
                >
            </div>
            {#if stats.top_system}
                <div class="intel-stat">
                    <span class="intel-stat-label">Hottest System</span>
                    <span class="intel-stat-value">{stats.top_system}</span>
                </div>
            {/if}
            {#if stats.top_pilot}
                <div class="intel-stat">
                    <span class="intel-stat-label">Top Pilot</span>
                    <span class="intel-stat-value">{stats.top_pilot}</span>
                </div>
            {/if}
        </div>
    {/if}

    <main class="intel-main">
        {#if channel}
            {#if view === "heatmap"}
                <Heatmap {channel} {weeks} />
            {:else if view === "safety"}
                <Safety {channel} {weeks} />
            {:else if view === "systems"}
                <Systems {channel} {weeks} />
            {:else if view === "pilots"}
                <Pilots {channel} />
            {:else if view === "current"}
                <Current {channel} />
            {/if}
        {:else}
            <div class="threats-empty">
                <div class="threats-empty-icon">📡</div>
                <div class="threats-empty-text">Loading channels…</div>
            </div>
        {/if}
    </main>
</div>
