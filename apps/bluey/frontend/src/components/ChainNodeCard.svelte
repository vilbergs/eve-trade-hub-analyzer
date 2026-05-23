<script lang="ts">
    import { Handle, Position, type NodeProps } from "@xyflow/svelte";
    import { getContext } from "svelte";
    import type { ChainNode, NodeKind } from "../lib/types";
    import type { GraphContext, InlineMaterial } from "./graph-context";

    type Data = {
        node: ChainNode;
        materials: InlineMaterial[];
        isFocal: boolean;
    };

    let { data }: NodeProps = $props();
    let d = $derived(data as unknown as Data);
    let node = $derived(d.node);
    let materials = $derived(d.materials);
    let isFocal = $derived(d.isFocal);

    const ctx = getContext<GraphContext>("bluey-graph");

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

    const MAT_CLASS: Record<NodeKind, string> = {
        raw_mineral: "mat-min",
        raw_moon: "mat-moon",
        pi: "mat-pi",
        reaction: "mat-react",
        component: "mat-comp",
        t1_item: "mat-t1",
        ram: "mat-ram",
        t2_product: "mat-t2",
        other: "mat-other",
    };

    function kindLabel(kind: NodeKind, typeId: number): string {
        switch (kind) {
            case "component":
                return "COMPONENT";
            case "t2_product":
                return "T2 BLUEPRINT";
            case "t1_item":
                return "ITEM";
            case "ram":
                return "ITEM";
            case "reaction":
                return "REACTION";
            case "pi": {
                const tier = ctx.piTierMap.get(typeId);
                return tier !== undefined ? `PI · P${tier}` : "PI";
            }
            case "raw_mineral":
                return "MINERAL";
            case "raw_moon":
                return "MOON GOO";
            default:
                return kind.toUpperCase();
        }
    }

    let nodeState = $derived(ctx.getNodeState(node.type_id));
    let isFocused = $derived(ctx.focusId === node.type_id);
    let isCollapsed = $derived(ctx.collapsedInputs.has(node.type_id));
    let qty = $derived(ctx.quantityMap.get(node.type_id));
</script>

<div
    class="fp-node {KIND_CLASS[node.kind] || ''}"
    class:is-on={nodeState === "build"}
    class:is-off={nodeState === "buy"}
    class:is-excluded={nodeState === "off"}
    class:is-focus={isFocal}
    class:is-node-focused={isFocused}
    onclick={() => ctx.setFocus(node.type_id)}
    role="button"
    tabindex="0"
    onkeydown={(e) => {
        if (e.key === "Enter") ctx.setFocus(node.type_id);
    }}
>
    <Handle type="target" position={Position.Left} class="fp-handle" />
    <Handle type="source" position={Position.Right} class="fp-handle" />

    <div class="fp-node-head">
        <div class="fp-node-kind-row">
            <div class="fp-node-kind">{kindLabel(node.kind, node.type_id)}</div>
            {#if qty != null && nodeState !== "off"}
                <div class="fp-node-qty">×{qty.toLocaleString()}</div>
            {/if}
        </div>
        <div class="fp-node-name">{node.name}</div>
    </div>

    {#if materials.length > 0}
        <div class="fp-recipe" class:is-collapsed={isCollapsed}>
            <button
                class="fp-recipe-toggle nodrag"
                onclick={(e) => {
                    e.stopPropagation();
                    ctx.toggleCollapse(node.type_id);
                }}
            >
                <span
                    class="fp-recipe-chevron"
                    class:is-open={!isCollapsed}>▸</span
                >
                Inputs
                <span class="fp-recipe-count">{materials.length}</span>
            </button>
            {#if !isCollapsed}
                <ul class="fp-recipe-list">
                    {#each materials as mat (mat.type_id)}
                        {@const piTier =
                            mat.kind === "pi"
                                ? ctx.piTierMap.get(mat.type_id)
                                : undefined}
                        <li
                            class="fp-recipe-row {MAT_CLASS[mat.kind] || ''}"
                        >
                            {#if piTier !== undefined}
                                <span class="fp-recipe-tier">P{piTier}</span>
                            {/if}
                            <span class="fp-recipe-name">{mat.name}</span>
                            <span class="fp-recipe-qty"
                                >×{mat.quantity.toLocaleString()}</span
                            >
                        </li>
                    {/each}
                </ul>
            {/if}
        </div>
    {/if}

    {#if node.has_recipe || !isFocal}
        <div class="fp-node-state nodrag" role="group">
            <button
                class="fp-state-label fp-state-label--build"
                class:is-current={nodeState === "build"}
                onclick={(e) => {
                    e.stopPropagation();
                    ctx.setNodeState(node.type_id, "build");
                }}>BUILD</button
            >
            <button
                class="fp-state-label fp-state-label--buy"
                class:is-current={nodeState === "buy"}
                onclick={(e) => {
                    e.stopPropagation();
                    ctx.setNodeState(node.type_id, "buy");
                }}>BUY</button
            >
            <button
                class="fp-state-label fp-state-label--off"
                class:is-current={nodeState === "off"}
                onclick={(e) => {
                    e.stopPropagation();
                    ctx.setNodeState(node.type_id, "off");
                }}>OFF</button
            >
        </div>
    {/if}
</div>
