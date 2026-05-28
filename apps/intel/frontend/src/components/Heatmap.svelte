<script lang="ts">
    import { fetchHeatmap } from "../lib/api";
    import type { HeatmapData } from "../lib/api";

    let { channel, weeks }: { channel: string; weeks: number } = $props();

    let data: HeatmapData | null = $state(null);
    let tooltip: { x: number; y: number; text: string } | null = $state(null);

    $effect(() => {
        if (channel) {
            fetchHeatmap(channel, weeks).then((d) => (data = d));
        }
    });

    function heatColor(value: number, observed: number): string {
        if (observed <= 0) return "var(--bg-2)";
        if (value <= 0) return "var(--heat-0)";
        if (value < 0.5) return "var(--heat-1)";
        if (value < 1.0) return "var(--heat-2)";
        if (value < 2.0) return "var(--heat-3)";
        if (value < 3.0) return "var(--heat-4)";
        if (value < 5.0) return "var(--heat-5)";
        if (value < 8.0) return "var(--heat-6)";
        return "var(--heat-7)";
    }

    function showTooltip(
        e: MouseEvent,
        wd: string,
        hour: number,
        value: number,
        observed: number,
    ) {
        const obs_h = observed / 60;
        tooltip = {
            x: e.clientX + 12,
            y: e.clientY - 8,
            text: `${wd} ${String(hour).padStart(2, "0")}:00 UTC — ${value.toFixed(2)} avg threats, ${obs_h.toFixed(1)}h observed`,
        };
    }

    function hideTooltip() {
        tooltip = null;
    }
</script>

<div class="heatmap-container">
    <h2 class="heatmap-title">Threat Heatmap</h2>
    <p class="heatmap-subtitle">
        Average number of simultaneously dirty systems per hour bucket · {weeks ===
        0
            ? "all time"
            : `${weeks} weeks`} · all times UTC
    </p>

    {#if data}
        <div class="heatmap-grid">
            <div class="heatmap-header"></div>
            {#each data.hours as h}
                <div class="heatmap-header">{String(h).padStart(2, "0")}</div>
            {/each}

            {#each data.weekdays as wd, wdIdx}
                <div class="heatmap-row-label">{wd}</div>
                {#each data.hours as h, hIdx}
                    {@const value = data.data[wdIdx][hIdx]}
                    {@const obs = data.observed[wdIdx][hIdx]}
                    <div
                        class="heatmap-cell"
                        class:no-data={obs <= 0}
                        style="background: {heatColor(value, obs)}"
                        onmouseenter={(e) => showTooltip(e, wd, h, value, obs)}
                        onmouseleave={hideTooltip}
                    >
                        {#if obs > 0 && value > 0}
                            {value.toFixed(1)}
                        {/if}
                    </div>
                {/each}
            {/each}
        </div>

        <div class="heatmap-legend">
            <span>Safe</span>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-0)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-1)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-2)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-3)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-4)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-5)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-6)"
            ></div>
            <div
                class="heatmap-legend-swatch"
                style="background: var(--heat-7)"
            ></div>
            <span>Dangerous</span>
            <span style="margin-left: 16px; color: var(--text-2)"
                >Grey = no observation data</span
            >
        </div>
    {:else}
        <div class="threats-empty">
            <div class="threats-empty-text">Loading heatmap…</div>
        </div>
    {/if}
</div>

{#if tooltip}
    <div class="tooltip" style="left: {tooltip.x}px; top: {tooltip.y}px">
        {tooltip.text}
    </div>
{/if}
