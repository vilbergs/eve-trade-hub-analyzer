<script lang="ts">
    import { fetchProducts } from "../lib/api";
    import type { ProductEntry, LedgerEntry } from "../lib/types";

    interface Props {
        ledger: LedgerEntry[];
        focusName: string | null;
        onAddProduct: (product: ProductEntry) => void;
        onRemoveProduct: (typeId: number) => void;
        onUpdateEntry: (
            typeId: number,
            field: "runs" | "me_percent",
            value: number,
        ) => void;
    }

    let {
        ledger,
        focusName,
        onAddProduct,
        onRemoveProduct,
        onUpdateEntry,
    }: Props = $props();

    // Autocomplete state
    let open = $state(false);
    let query = $state("");
    let options: ProductEntry[] = $state([]);
    let hi = $state(0);
    let rootEl: HTMLDivElement | undefined = $state(undefined);
    let inputEl: HTMLInputElement | undefined = $state(undefined);

    // Debounced search
    $effect(() => {
        if (!open) return;
        const q = query;
        const timer = setTimeout(() => {
            fetchProducts(q)
                .then((r) => {
                    options = r;
                })
                .catch(console.error);
        }, 150);
        return () => clearTimeout(timer);
    });

    // Close on outside click
    $effect(() => {
        if (!open) return;
        function onDocClick(e: MouseEvent) {
            if (rootEl && !rootEl.contains(e.target as Node)) open = false;
        }
        document.addEventListener("mousedown", onDocClick);
        return () => document.removeEventListener("mousedown", onDocClick);
    });

    function pick(o: ProductEntry) {
        onAddProduct(o);
        open = false;
        query = "";
        inputEl?.blur();
    }

    function onKeyDown(e: KeyboardEvent) {
        if (e.key === "ArrowDown") {
            e.preventDefault();
            hi = Math.min(options.length - 1, hi + 1);
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            hi = Math.max(0, hi - 1);
        } else if (e.key === "Enter") {
            e.preventDefault();
            if (options[hi]) pick(options[hi]);
        } else if (e.key === "Escape") {
            open = false;
            query = "";
            inputEl?.blur();
        }
    }

    // Check if a product is already in the ledger
    function isInLedger(typeId: number): boolean {
        return ledger.some((e) => e.type_id === typeId);
    }
</script>

<header class="fp-header">
    <div class="fp-brand">
        <div class="fp-brand-mark">
            <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
                <path
                    d="M4 8 L14 3 L24 8 L24 20 L14 25 L4 20 Z"
                    stroke="currentColor"
                    stroke-width="1.5"
                />
                <path
                    d="M14 3 L14 25 M4 8 L24 20 M24 8 L4 20"
                    stroke="currentColor"
                    stroke-width="1"
                    opacity=".5"
                />
                <circle cx="14" cy="14" r="3" fill="currentColor" />
            </svg>
        </div>
        <div class="fp-brand-text">
            <div class="fp-brand-name">BLUEY</div>
            <div class="fp-brand-sub">industry planner</div>
        </div>
    </div>

    <div class="fp-header-divider"></div>

    <div class="fp-product-picker">
        <div class="fp-picker-eyebrow">ADD BLUEPRINT</div>
        <div class="fp-autoc" bind:this={rootEl}>
            <input
                bind:this={inputEl}
                class="fp-autoc-input"
                type="text"
                value={query}
                placeholder="Search blueprints…"
                oninput={(e) => {
                    query = (e.target as HTMLInputElement).value;
                    hi = 0;
                    if (!open) open = true;
                }}
                onfocus={() => {
                    open = true;
                    hi = 0;
                }}
                onkeydown={onKeyDown}
                autocomplete="off"
                spellcheck="false"
            />
            <span class="fp-autoc-caret" aria-hidden="true">+</span>
            {#if open && options.length === 0 && query.length > 0}
                <div class="fp-autoc-menu fp-autoc-no-results">No results</div>
            {:else if open && options.length > 0}
                <ul class="fp-autoc-menu" role="listbox">
                    {#each options as o, i}
                        <li
                            role="option"
                            aria-selected={i === hi}
                            class="fp-autoc-opt"
                            class:is-hi={i === hi}
                            class:is-sel={isInLedger(o.type_id)}
                            onmousedown={(e) => {
                                e.preventDefault();
                                pick(o);
                            }}
                            onmouseenter={() => {
                                hi = i;
                            }}
                        >
                            <span class="fp-autoc-opt-name">{o.name}</span>
                            <span class="fp-autoc-opt-group"
                                >{o.group_name}</span
                            >
                            {#if isInLedger(o.type_id)}
                                <span class="fp-autoc-opt-flag">added</span>
                            {/if}
                        </li>
                    {/each}
                </ul>
            {/if}
        </div>
    </div>

    <div class="fp-header-divider"></div>

    <div class="fp-ledger">
        <div class="fp-ledger-eyebrow">
            BUILD PLAN <span class="fp-ledger-count">{ledger.length}</span>
        </div>
        {#if ledger.length === 0}
            <div class="fp-ledger-empty">Add blueprints to begin</div>
        {:else}
            <div class="fp-ledger-list">
                {#each ledger as entry (entry.type_id)}
                    <div class="fp-ledger-entry">
                        <span class="fp-ledger-name" title={entry.name}
                            >{entry.name}</span
                        >
                        <div class="fp-ledger-controls">
                            <label class="fp-ledger-field">
                                <span class="fp-ledger-field-k">R</span>
                                <input
                                    type="number"
                                    min="1"
                                    max="9999"
                                    value={entry.runs}
                                    oninput={(e) =>
                                        onUpdateEntry(
                                            entry.type_id,
                                            "runs",
                                            Math.max(
                                                1,
                                                Math.min(
                                                    9999,
                                                    +(
                                                        e.target as HTMLInputElement
                                                    ).value || 1,
                                                ),
                                            ),
                                        )}
                                />
                            </label>
                            <label class="fp-ledger-field">
                                <span class="fp-ledger-field-k">ME</span>
                                <button
                                    class="fp-ledger-me-btn"
                                    onclick={() =>
                                        onUpdateEntry(
                                            entry.type_id,
                                            "me_percent",
                                            Math.max(0, entry.me_percent - 1),
                                        )}>−</button
                                >
                                <span class="fp-ledger-me-val"
                                    >{entry.me_percent}%</span
                                >
                                <button
                                    class="fp-ledger-me-btn"
                                    onclick={() =>
                                        onUpdateEntry(
                                            entry.type_id,
                                            "me_percent",
                                            Math.min(10, entry.me_percent + 1),
                                        )}>+</button
                                >
                            </label>
                        </div>
                        <button
                            class="fp-ledger-remove"
                            onclick={() => onRemoveProduct(entry.type_id)}
                            title="Remove from plan">×</button
                        >
                    </div>
                {/each}
            </div>
        {/if}
    </div>

    <div class="fp-header-divider"></div>
    <div class="fp-focus-card">
        <div class="fp-focus-eyebrow">CURRENT FOCUS</div>
        {#if focusName}
            <div class="fp-focus-name">{focusName}</div>
            <div class="fp-focus-sub">
                click any node to re-center the graph
            </div>
        {:else}
            <div class="fp-focus-name fp-focus-name--dim">—</div>
            <div class="fp-focus-sub">
                click any node to re-center the graph
            </div>
        {/if}
    </div>
</header>
