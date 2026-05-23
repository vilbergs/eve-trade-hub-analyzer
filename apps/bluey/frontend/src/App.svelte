<script lang="ts">
    import Header from "./components/Header.svelte";
    import Legend from "./components/Legend.svelte";
    import Graph from "./components/Graph.svelte";
    import BomPanel from "./components/BomPanel.svelte";
    import { fetchChain, fetchMultiBom, mergeChains } from "./lib/api";
    import type {
        ChainResponse,
        BomResponse,
        MergedChain,
        LedgerEntry,
        ProductEntry,
    } from "./lib/types";

    const STORAGE_KEY = "bluey:ledger";

    function loadLedger(): LedgerEntry[] {
        try {
            const raw = localStorage.getItem(STORAGE_KEY);
            if (!raw) return [];
            const parsed = JSON.parse(raw);
            if (Array.isArray(parsed)) return parsed;
        } catch {}
        return [];
    }

    function saveLedger(entries: LedgerEntry[]) {
        try {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
        } catch {}
    }

    let ledger: LedgerEntry[] = $state(loadLedger());
    let focusName: string | null = $state(null);
    let mergedChain: MergedChain | null = $state(null);
    let bom: BomResponse | null = $state(null);
    let activeSet: Set<number> = $state(new Set());
    let excludedSet: Set<number> = $state(new Set());
    let hiddenKinds: Set<string> = $state(new Set());
    let loading = $state(false);
    let generation = 0;

    // Derived: only changes when the set of type_ids in the ledger changes
    let ledgerTypeIds = $derived(
        ledger
            .map((e) => e.type_id)
            .sort()
            .join(","),
    );

    function toggleKind(kind: string) {
        const next = new Set(hiddenKinds);
        if (next.has(kind)) next.delete(kind);
        else next.add(kind);
        hiddenKinds = next;
    }

    // Persist ledger to localStorage on every change
    $effect(() => {
        saveLedger(ledger);
    });

    // Track fetched chains per type_id to avoid refetching
    let chainCache = new Map<number, ChainResponse>();

    function addProduct(product: ProductEntry) {
        if (ledger.some((e) => e.type_id === product.type_id)) return;
        ledger = [
            ...ledger,
            {
                type_id: product.type_id,
                name: product.name,
                group_name: product.group_name,
                category_name: product.category_name,
                runs: 10,
                me_percent: 10,
            },
        ];
    }

    function removeProduct(typeId: number) {
        const removed = ledger.find((e) => e.type_id === typeId);
        ledger = ledger.filter((e) => e.type_id !== typeId);
        chainCache.delete(typeId);

        // Clear focusName if it was the removed product
        if (removed && focusName === removed.name) {
            focusName = ledger.length > 0 ? ledger[0].name : null;
        }
    }

    function updateEntry(
        typeId: number,
        field: "runs" | "me_percent",
        value: number,
    ) {
        ledger = ledger.map((e) =>
            e.type_id === typeId ? { ...e, [field]: value } : e,
        );
    }

    // Set a node to a specific state: build / buy / off
    function setNodeState(typeId: number, state: "build" | "buy" | "off") {
        const nextActive = new Set(activeSet);
        const nextExcluded = new Set(excludedSet);

        // Remove from both sets first
        nextActive.delete(typeId);
        nextExcluded.delete(typeId);

        // Then add to the appropriate set
        if (state === "build") {
            nextActive.add(typeId);
        } else if (state === "off") {
            nextExcluded.add(typeId);
        }

        activeSet = nextActive;
        excludedSet = nextExcluded;
    }

    // Fetch and merge chains only when the set of type_ids changes
    $effect(() => {
        // Subscribe to ledgerTypeIds so this only re-runs when products are added/removed
        const _typeIds = ledgerTypeIds;
        const entries = ledger;

        if (entries.length === 0) {
            mergedChain = null;
            activeSet = new Set();
            bom = null;
            focusName = null;
            return;
        }

        const gen = ++generation;
        loading = true;

        // Fetch chains we don't have cached yet
        const toFetch = entries.filter((e) => !chainCache.has(e.type_id));
        const fetchPromises = toFetch.map((e) =>
            fetchChain(e.type_id).then((chain) => {
                chainCache.set(e.type_id, chain);
            }),
        );

        Promise.all(fetchPromises)
            .then(() => {
                if (gen !== generation) return; // stale

                // Merge all cached chains for current ledger entries
                const chains = entries
                    .map((e) => chainCache.get(e.type_id))
                    .filter((c): c is ChainResponse => c != null);

                const merged = mergeChains(chains);
                mergedChain = merged;

                // Only add newly-appeared focal products to activeSet
                const nextActive = new Set(activeSet);
                for (const fid of merged.focal_type_ids) {
                    // Only add if it's not already tracked in either set
                    if (!nextActive.has(fid) && !excludedSet.has(fid)) {
                        nextActive.add(fid);
                    }
                }
                activeSet = nextActive;

                // Set focus to first product name if not set
                if (!focusName && entries.length > 0) {
                    focusName = entries[0].name;
                }
            })
            .catch((e) => console.error(e))
            .finally(() => {
                if (gen === generation) {
                    loading = false;
                }
            });
    });

    // Generation counter for BOM fetches
    let bomGeneration = 0;

    // Recompute BOM when ledger entries or activeSet change
    $effect(() => {
        const entries = ledger;
        if (entries.length === 0) {
            bom = null;
            return;
        }
        // We need the chain to be loaded first
        if (!mergedChain) return;

        const built = Array.from(activeSet);
        const excluded = excludedSet;
        const hidden = hiddenKinds;

        // Build a set of type_ids whose kind is hidden
        const hiddenTypeIds = new Set<number>();
        if (hidden.size > 0 && mergedChain) {
            for (const node of mergedChain.nodes) {
                if (hidden.has(node.kind)) {
                    hiddenTypeIds.add(node.type_id);
                }
            }
        }

        const bomGen = ++bomGeneration;

        fetchMultiBom(entries, built)
            .then((data) => {
                if (bomGen !== bomGeneration) return; // stale

                // Filter out excluded and hidden items from BOM
                const shouldHide = (typeId: number) =>
                    excluded.has(typeId) || hiddenTypeIds.has(typeId);

                if (excluded.size > 0 || hiddenTypeIds.size > 0) {
                    const filteredBuy = data.buy.filter(
                        (l) => !shouldHide(l.type_id),
                    );
                    const filteredBuild = data.build.filter(
                        (l) => !shouldHide(l.type_id),
                    );
                    const removedCost = [
                        ...data.buy.filter((l) => shouldHide(l.type_id)),
                        ...data.build.filter((l) => shouldHide(l.type_id)),
                    ].reduce((sum, l) => sum + (l.line_cost ?? 0), 0);
                    bom = {
                        buy: filteredBuy,
                        build: filteredBuild,
                        total_cost: data.total_cost - removedCost,
                    };
                } else {
                    bom = data;
                }
            })
            .catch((e) => console.error(e));
    });
</script>

<div class="fp-app">
    <Header
        {ledger}
        {focusName}
        onAddProduct={addProduct}
        onRemoveProduct={removeProduct}
        onUpdateEntry={updateEntry}
    />
    <Legend {hiddenKinds} onToggleKind={toggleKind} />
    <main class="fp-main">
        {#if loading}
            <div class="fp-loading">Loading chain…</div>
        {/if}
        {#if mergedChain}
            <Graph
                chain={mergedChain}
                {activeSet}
                {excludedSet}
                {hiddenKinds}
                {setNodeState}
                {bom}
                {ledger}
                onFocusChange={(_id, name) => {
                    focusName = name;
                }}
            />
        {:else if !loading}
            <div class="fp-loading">Select a blueprint to begin</div>
        {/if}
        <BomPanel
            {bom}
            ledgerCount={ledger.length}
            activeCount={activeSet.size}
        />
    </main>
    <footer class="fp-footer">
        <div class="fp-foot-info">
            <span>JITA 4-4</span>
            <span class="fp-foot-sep">·</span>
            <span>prices indicative</span>
        </div>
        <div class="fp-foot-canvas-hint">
            <span class="kbd">drag</span> to pan
            <span class="fp-foot-sep">·</span>
            <span class="kbd">scroll</span> to zoom
            <span class="fp-foot-sep">·</span>
            <span class="kbd">click</span> a node to refocus
            <span class="fp-foot-sep">·</span>
            toggle to add to BOM
        </div>
    </footer>
</div>
