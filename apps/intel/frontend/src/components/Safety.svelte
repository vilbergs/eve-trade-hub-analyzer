<script lang="ts">
    import { fetchSafety } from "../lib/api";
    import type { SafetyData } from "../lib/api";

    let { channel, weeks }: { channel: string; weeks: number } = $props();

    let data: SafetyData | null = $state(null);
    let tooltip: { x: number; y: number; text: string } | null = $state(null);

    $effect(() => {
        if (channel) {
            fetchSafety(channel, weeks).then((d) => (data = d));
        }
    });

    function cellColor(pct: number, observed: number): string {
        if (observed <= 0) return "var(--bg-2)";
        if (pct <= 0) return "var(--heat-0)";
        if (pct < 5) return "var(--heat-1)";
        if (pct < 10) return "var(--heat-2)";
        if (pct < 20) return "var(--heat-3)";
        if (pct < 35) return "var(--heat-4)";
        if (pct < 50) return "var(--heat-5)";
        if (pct < 75) return "var(--heat-6)";
        return "var(--heat-7)";
    }

    function showTooltip(
        e: MouseEvent,
        system: string,
        hour: number,
        pct: number,
        observed: number,
    ) {
        tooltip = {
            x: e.clientX + 12,
            y: e.clientY - 8,
            text: `${system} @ ${String(hour).padStart(2, "0")}:00 — ${pct.toFixed(1)}% dirty, ${(observed / 60).toFixed(1)}h observed`,
        };
    }

    function hideTooltip() {
        tooltip = null;
    }
</script>

<div>
    <h2 class="section-title">System Safety Matrix</h2>
    <p class="section-subtitle">
        Percentage of observed time each system was reported dirty · {weeks ===
        0
            ? "all time"
            : `${weeks} weeks`} · all times UTC
    </p>

    {#if data}
        <div class="safety-grid">
            <div class="safety-header"></div>
            {#each Array(24) as _, h}
                <div class="safety-header">{String(h).padStart(2, "0")}</div>
            {/each}

            {#each data.systems as sys}
                <div class="safety-system" title={sys.name}>{sys.name}</div>
                {#each sys.buckets as pct, h}
                    {@const obs = data.observed_hours[h]}
                    <div
                        class="safety-cell"
                        style="background: {cellColor(pct, obs)}"
                        onmouseenter={(e) =>
                            showTooltip(e, sys.name, h, pct, obs)}
                        onmouseleave={hideTooltip}
                    >
                        {#if obs > 0 && pct >= 1}
                            {pct.toFixed(0)}
                        {/if}
                    </div>
                {/each}
            {/each}
        </div>
    {:else}
        <div class="threats-empty">
            <div class="threats-empty-text">Loading safety data…</div>
        </div>
    {/if}
</div>

{#if tooltip}
    <div class="tooltip" style="left: {tooltip.x}px; top: {tooltip.y}px">
        {tooltip.text}
    </div>
{/if}
