<script lang="ts">
    import { computePiTiers } from "../lib/pi-tier";
    import type {
        MergedChain,
        ChainNode,
        NodeKind,
        BomResponse,
        LedgerEntry,
    } from "../lib/types";

    interface Props {
        chain: MergedChain;
        activeSet: Set<number>;
        excludedSet: Set<number>;
        setNodeState: (typeId: number, state: "build" | "buy" | "off") => void;
        bom: BomResponse | null;
        ledger: LedgerEntry[];
        hiddenKinds?: Set<string>;
    }

    let {
        chain,
        activeSet,
        excludedSet,
        setNodeState,
        bom,
        ledger,
        hiddenKinds = new Set(),
    }: Props = $props();

    type NodeState = "build" | "buy" | "off";
    function getNodeState(typeId: number): NodeState {
        if (excludedSet.has(typeId)) return "off";
        if (activeSet.has(typeId)) return "build";
        return "buy";
    }

    let focalSet = $derived(new Set(chain.focal_type_ids));
    let piTierMap = $derived(computePiTiers(chain));

    let quantityMap = $derived.by(() => {
        const map = new Map<number, number>();
        for (const entry of ledger) map.set(entry.type_id, entry.runs);
        if (bom) {
            for (const line of bom.buy) map.set(line.type_id, line.quantity);
            for (const line of bom.build)
                map.set(line.type_id, line.quantity);
        }
        return map;
    });

    let priceMap = $derived.by(() => {
        const map = new Map<
            number,
            { unit_price: number | null; line_cost: number | null }
        >();
        if (bom) {
            for (const line of bom.buy)
                map.set(line.type_id, {
                    unit_price: line.unit_price,
                    line_cost: line.line_cost,
                });
            for (const line of bom.build)
                map.set(line.type_id, {
                    unit_price: line.unit_price,
                    line_cost: line.line_cost,
                });
        }
        return map;
    });

    type SectionKey =
        | "t2_product"
        | "t1_item"
        | "component"
        | "ram"
        | "reaction"
        | "pi_p4"
        | "pi_p3"
        | "pi_p2"
        | "pi_p1"
        | "pi_p0"
        | "raw_moon"
        | "raw_mineral"
        | "other";

    interface Section {
        key: SectionKey;
        label: string;
        nodes: ChainNode[];
    }

    function sectionKeyFor(node: ChainNode): SectionKey {
        if (node.kind === "pi") {
            const tier = piTierMap.get(node.type_id);
            if (tier === 0) return "pi_p0";
            if (tier === 1) return "pi_p1";
            if (tier === 2) return "pi_p2";
            if (tier === 3) return "pi_p3";
            if (tier === 4) return "pi_p4";
        }
        switch (node.kind) {
            case "t2_product":
                return "t2_product";
            case "t1_item":
                return "t1_item";
            case "component":
                return "component";
            case "ram":
                return "ram";
            case "reaction":
                return "reaction";
            case "raw_moon":
                return "raw_moon";
            case "raw_mineral":
                return "raw_mineral";
            default:
                return "other";
        }
    }

    const SECTION_ORDER: { key: SectionKey; label: string }[] = [
        { key: "t2_product", label: "T2 PRODUCT" },
        { key: "t1_item", label: "T1 ITEM" },
        { key: "component", label: "COMPONENT" },
        { key: "ram", label: "R.A.M." },
        { key: "reaction", label: "REACTION" },
        { key: "pi_p4", label: "PI · P4" },
        { key: "pi_p3", label: "PI · P3" },
        { key: "pi_p2", label: "PI · P2" },
        { key: "pi_p1", label: "PI · P1" },
        { key: "pi_p0", label: "PI · P0" },
        { key: "raw_moon", label: "MOON GOO" },
        { key: "raw_mineral", label: "MINERAL" },
        { key: "other", label: "OTHER" },
    ];

    let sections = $derived.by(() => {
        const groups = new Map<SectionKey, ChainNode[]>();
        for (const node of chain.nodes) {
            if (hiddenKinds.has(node.kind) && !focalSet.has(node.type_id))
                continue;
            const key = sectionKeyFor(node);
            const list = groups.get(key) ?? [];
            list.push(node);
            groups.set(key, list);
        }
        // Sort each group by name
        for (const list of groups.values()) {
            list.sort((a, b) => a.name.localeCompare(b.name));
        }
        const out: Section[] = [];
        for (const { key, label } of SECTION_ORDER) {
            const list = groups.get(key);
            if (list && list.length > 0) out.push({ key, label, nodes: list });
        }
        return out;
    });

    const KIND_CLASS: Record<NodeKind, string> = {
        raw_mineral: "n-raw-min",
        raw_moon: "n-raw-moon",
        pi: "n-pi",
        reaction: "n-react",
        component: "n-comp",
        t1_item: "n-t1",
        ram: "n-ram",
        t2_product: "n-t2",
        other: "",
    };

    function formatIsk(n: number | null | undefined): string {
        if (n == null) return "—";
        if (n >= 1_000_000_000)
            return (n / 1_000_000_000).toFixed(2) + "B";
        if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
        if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
        return n.toFixed(0);
    }

    // Whether this row's state buttons should be rendered
    function showStateButtons(node: ChainNode): boolean {
        // Raw mineral/moon: no recipe to build, no toggle
        if (node.kind === "raw_mineral" || node.kind === "raw_moon")
            return false;
        // Focal products without a recipe: also nothing to toggle
        if (focalSet.has(node.type_id) && !node.has_recipe) return false;
        return true;
    }
</script>

<div class="fp-breakdown">
    {#each sections as section (section.key)}
        <section class="fp-bd-section">
            <header class="fp-bd-head">
                <span class="fp-bd-label">{section.label}</span>
                <span class="fp-bd-count">{section.nodes.length}</span>
            </header>
            <ul class="fp-bd-list">
                {#each section.nodes as node (node.type_id)}
                    {@const state = getNodeState(node.type_id)}
                    {@const qty = quantityMap.get(node.type_id)}
                    {@const price = priceMap.get(node.type_id)}
                    <li
                        class="fp-bd-row {KIND_CLASS[node.kind] || ''}"
                        class:is-on={state === "build"}
                        class:is-off={state === "buy"}
                        class:is-excluded={state === "off"}
                    >
                        <span class="fp-bd-name">{node.name}</span>
                        <span class="fp-bd-qty"
                            >{qty != null ? `×${qty.toLocaleString()}` : ""}
                        </span>
                        <span class="fp-bd-price"
                            >{price?.unit_price != null
                                ? formatIsk(price.unit_price)
                                : "—"}</span
                        >
                        <span class="fp-bd-cost"
                            >{price?.line_cost != null
                                ? formatIsk(price.line_cost)
                                : "—"}</span
                        >
                        {#if showStateButtons(node)}
                            <div class="fp-bd-state" role="group">
                                <button
                                    class="fp-state-label fp-state-label--build"
                                    class:is-current={state === "build"}
                                    onclick={() =>
                                        setNodeState(node.type_id, "build")}
                                    >BUILD</button
                                >
                                <button
                                    class="fp-state-label fp-state-label--buy"
                                    class:is-current={state === "buy"}
                                    onclick={() =>
                                        setNodeState(node.type_id, "buy")}
                                    >BUY</button
                                >
                                <button
                                    class="fp-state-label fp-state-label--off"
                                    class:is-current={state === "off"}
                                    onclick={() =>
                                        setNodeState(node.type_id, "off")}
                                    >OFF</button
                                >
                            </div>
                        {:else}
                            <div class="fp-bd-state-placeholder"></div>
                        {/if}
                    </li>
                {/each}
            </ul>
        </section>
    {/each}
</div>
