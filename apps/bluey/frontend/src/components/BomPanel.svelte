<script lang="ts">
    import type { BomResponse, BomLineWithPrice } from "../lib/types";

    interface Props {
        bom: BomResponse | null;
        ledgerCount: number;
        activeCount: number;
    }

    let { bom, ledgerCount, activeCount }: Props = $props();

    function fmtISK(n: number): string {
        if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
        if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
        if (n >= 1e3) return (n / 1e3).toFixed(1) + "K";
        return Math.round(n).toLocaleString();
    }

    function fmtQty(n: number): string {
        return n.toLocaleString();
    }

    // Memoized sorted lists
    let sortedBuy = $derived(
        bom
            ? [...bom.buy].sort(
                  (a, b) => (b.line_cost ?? 0) - (a.line_cost ?? 0),
              )
            : [],
    );
    let sortedBuild = $derived(
        bom
            ? [...bom.build].sort(
                  (a, b) => (b.line_cost ?? 0) - (a.line_cost ?? 0),
              )
            : [],
    );

    let copied = $state(false);

    function copyMultiBuy() {
        if (!bom) return;
        const text = bom.buy
            .map((l) => `${l.name || `Type ${l.type_id}`} ${l.quantity}`)
            .join("\n");
        navigator.clipboard.writeText(text).then(() => {
            copied = true;
            setTimeout(() => {
                copied = false;
            }, 2000);
        });
    }
</script>

<aside class="fp-bom">
    <header class="fp-bom-head">
        <div class="fp-bom-eyebrow">BILL OF MATERIALS</div>
        <div class="fp-bom-title">Shopping list</div>
        <div class="fp-bom-sub">
            {#if bom}
                Aggregated across {activeCount} active node{activeCount === 1
                    ? ""
                    : "s"} · expanded to raw inputs
            {:else}
                Select a blueprint to begin
            {/if}
        </div>
    </header>

    {#if bom}
        <div class="fp-bom-stats">
            <div class="fp-stat">
                <div class="fp-stat-k">Products</div>
                <div class="fp-stat-v">{ledgerCount}</div>
            </div>
            <div class="fp-stat">
                <div class="fp-stat-k">Active</div>
                <div class="fp-stat-v">{activeCount}</div>
            </div>
            <div class="fp-stat">
                <div class="fp-stat-k">Est. cost</div>
                <div class="fp-stat-v fp-stat-isk">
                    {fmtISK(bom.total_cost)} <span>ISK</span>
                </div>
            </div>
            <div class="fp-stat">
                <div class="fp-stat-k">Volume</div>
                <div class="fp-stat-v">—<span>m³</span></div>
            </div>
        </div>

        {#if bom.buy.length > 0}
            <section class="fp-bom-group">
                <header class="fp-bom-group-head">
                    <span class="fp-bom-group-name">Buy</span>
                    <span class="fp-bom-group-count"
                        >{bom.buy.length} items</span
                    >
                </header>
                <ul>
                    {#each sortedBuy as line (line.type_id)}
                        <li class="fp-bom-row">
                            <div class="fp-bom-row-name">
                                <span class="fp-bom-row-full"
                                    >{line.name || `Type ${line.type_id}`}</span
                                >
                            </div>
                            <div class="fp-bom-row-qty">
                                {fmtQty(line.quantity)}
                            </div>
                            <div class="fp-bom-row-cost">
                                {line.line_cost != null
                                    ? fmtISK(line.line_cost)
                                    : "—"}
                            </div>
                        </li>
                    {/each}
                </ul>
            </section>
        {/if}

        {#if bom.build.length > 0}
            <section class="fp-bom-group">
                <header class="fp-bom-group-head">
                    <span class="fp-bom-group-name">Build</span>
                    <span class="fp-bom-group-count"
                        >{bom.build.length} items</span
                    >
                </header>
                <ul>
                    {#each sortedBuild as line (line.type_id)}
                        <li class="fp-bom-row">
                            <div class="fp-bom-row-name">
                                <span class="fp-bom-row-full"
                                    >{line.name || `Type ${line.type_id}`}</span
                                >
                            </div>
                            <div class="fp-bom-row-qty">
                                {fmtQty(line.quantity)}
                            </div>
                            <div class="fp-bom-row-cost">
                                {line.line_cost != null
                                    ? fmtISK(line.line_cost)
                                    : "—"}
                            </div>
                        </li>
                    {/each}
                </ul>
            </section>
        {/if}

        <footer class="fp-bom-foot">
            <button class="fp-btn fp-btn-ghost" onclick={copyMultiBuy}>
                {copied ? "Copied!" : "Copy multi-buy"}
            </button>
        </footer>
    {:else}
        <div class="fp-bom-empty">
            <div class="fp-bom-empty-glyph">⌀</div>
            <div class="fp-bom-empty-title">No product selected</div>
            <div class="fp-bom-empty-sub">
                Search for a blueprint above to see its manufacturing chain and
                BOM.
            </div>
        </div>
    {/if}
</aside>
