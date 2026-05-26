<script lang="ts">
    import {
        SvelteFlow,
        Background,
        Controls,
        Panel,
        type Node,
        type Edge,
    } from "@xyflow/svelte";
    import { setContext } from "svelte";
    import { computePiTiers } from "../lib/pi-tier";
    import type {
        MergedChain,
        ChainNode,
        NodeKind,
        BomResponse,
        LedgerEntry,
    } from "../lib/types";
    import type {
        GraphContext,
        InlineMaterial,
        NodeState,
    } from "./graph-context";
    import ChainNodeCard from "./ChainNodeCard.svelte";

    interface Props {
        chain: MergedChain;
        activeSet: Set<number>;
        excludedSet: Set<number>;
        setNodeState: (typeId: number, state: NodeState) => void;
        onFocusChange?: (typeId: number | null, name: string | null) => void;
        bom: BomResponse | null;
        ledger: LedgerEntry[];
        hiddenKinds?: Set<string>;
    }

    let {
        chain,
        activeSet,
        excludedSet,
        setNodeState,
        onFocusChange,
        bom,
        ledger,
        hiddenKinds = new Set(),
    }: Props = $props();

    // ── Local UI state ───────────────────────────────────────────
    let collapsedInputs: Set<number> = $state(new Set());
    let focusId: number | null = $state(null);

    function toggleCollapse(typeId: number) {
        const next = new Set(collapsedInputs);
        if (next.has(typeId)) next.delete(typeId);
        else next.add(typeId);
        collapsedInputs = next;
    }

    function collapseAll() {
        const next = new Set<number>();
        for (const [id, mats] of inlineMaterialsMap) {
            if (mats.length > 0) next.add(id);
        }
        collapsedInputs = next;
    }

    function expandAll() {
        collapsedInputs = new Set();
    }

    // "Any expanded" → next click collapses all; otherwise expands all
    let anyExpanded = $derived.by(() => {
        for (const [id, mats] of inlineMaterialsMap) {
            if (mats.length > 0 && !collapsedInputs.has(id)) return true;
        }
        return false;
    });

    function getNodeState(typeId: number): NodeState {
        if (excludedSet.has(typeId)) return "off";
        if (activeSet.has(typeId)) return "build";
        return "buy";
    }

    let nodeMap = $derived(new Map(chain.nodes.map((n) => [n.type_id, n])));

    function setFocus(typeId: number) {
        if (focusId === typeId) {
            focusId = null;
            onFocusChange?.(null, null);
        } else {
            focusId = typeId;
            const node = nodeMap.get(typeId);
            onFocusChange?.(typeId, node?.name ?? null);
        }
    }

    // ── Quantity map (from BOM + ledger) ─────────────────────────
    let quantityMap = $derived.by(() => {
        const map = new Map<number, number>();
        for (const entry of ledger) {
            map.set(entry.type_id, entry.runs);
        }
        if (bom) {
            for (const line of bom.buy) {
                map.set(line.type_id, line.quantity);
            }
            for (const line of bom.build) {
                map.set(line.type_id, line.quantity);
            }
        }
        return map;
    });

    let focalSet = $derived(new Set(chain.focal_type_ids));

    function isRaw(node: ChainNode): boolean {
        if (node.kind === "raw_mineral" || node.kind === "raw_moon")
            return true;
        if (node.kind === "other" && !node.has_recipe) return true;
        return false;
    }

    let piTierMap = $derived(computePiTiers(chain));

    // ── Expose context to ChainNodeCard ──────────────────────────
    const ctx: GraphContext = {
        get focusId() {
            return focusId;
        },
        get collapsedInputs() {
            return collapsedInputs;
        },
        get quantityMap() {
            return quantityMap;
        },
        get piTierMap() {
            return piTierMap;
        },
        getNodeState,
        setNodeState: (id, s) => setNodeState(id, s),
        setFocus,
        toggleCollapse,
    };
    setContext("bluey-graph", ctx);

    // ── Compute the set of nodes worth showing ───────────────────
    // A node is "needed" iff some downstream chain ends at a focal product
    // through nodes the user has set to "build". A buy/off material that
    // only feeds other buy/off nodes is dead weight — hide it.
    let neededIds = $derived.by(() => {
        const needed = new Set<number>(chain.focal_type_ids);
        // Build nodes are always shown (user explicitly chose to manufacture)
        for (const id of activeSet) needed.add(id);

        // Add buy nodes that directly feed a visible build/focal consumer.
        // Iterate to a fixed point — a buy node never recurses, but newly-added
        // build nodes (already in activeSet) would have been pre-seeded above.
        let added = true;
        while (added) {
            added = false;
            for (const e of chain.edges) {
                if (!needed.has(e.to_type_id)) continue;
                // Only build/focal consumers pull in their inputs
                if (!focalSet.has(e.to_type_id) && !activeSet.has(e.to_type_id))
                    continue;
                const inputId = e.from_type_id;
                if (needed.has(inputId)) continue;
                if (excludedSet.has(inputId)) continue;
                needed.add(inputId);
                added = true;
            }
        }
        return needed;
    });

    // ── Depth-based columns (same logic as before) ───────────────
    let columns = $derived.by(() => {
        const nonRawNodes: ChainNode[] = [];
        const nonRawIds = new Set<number>();
        for (const node of chain.nodes) {
            if (isRaw(node)) continue;
            if (
                hiddenKinds.size > 0 &&
                hiddenKinds.has(node.kind) &&
                !focalSet.has(node.type_id)
            )
                continue;
            if (!neededIds.has(node.type_id)) continue;
            nonRawNodes.push(node);
            nonRawIds.add(node.type_id);
        }

        const childrenOf = new Map<number, number[]>();
        for (const edge of chain.edges) {
            if (
                !nonRawIds.has(edge.from_type_id) ||
                !nonRawIds.has(edge.to_type_id)
            )
                continue;
            const list = childrenOf.get(edge.from_type_id) ?? [];
            list.push(edge.to_type_id);
            childrenOf.set(edge.from_type_id, list);
        }

        const depth = new Map<number, number>();
        for (const fid of chain.focal_type_ids) depth.set(fid, 0);

        let changed = true;
        let iterations = 0;
        while (changed && iterations < 50) {
            changed = false;
            iterations++;
            for (const node of nonRawNodes) {
                if (focalSet.has(node.type_id)) continue;
                const consumers = childrenOf.get(node.type_id);
                if (!consumers || consumers.length === 0) {
                    if (!depth.has(node.type_id)) {
                        depth.set(node.type_id, 1);
                        changed = true;
                    }
                    continue;
                }
                let maxConsumerDepth = -1;
                for (const cid of consumers) {
                    const cd = depth.get(cid);
                    if (cd !== undefined && cd > maxConsumerDepth)
                        maxConsumerDepth = cd;
                }
                if (maxConsumerDepth >= 0) {
                    const newDepth = maxConsumerDepth + 1;
                    const existing = depth.get(node.type_id);
                    if (existing === undefined || newDepth > existing) {
                        depth.set(node.type_id, newDepth);
                        changed = true;
                    }
                }
            }
        }

        for (const node of nonRawNodes) {
            if (!depth.has(node.type_id)) depth.set(node.type_id, 1);
        }

        const maxDepth = Math.max(...depth.values(), 0);
        const cols: ChainNode[][] = [];
        for (let d = maxDepth; d >= 0; d--) {
            const col: ChainNode[] = [];
            for (const node of nonRawNodes) {
                if (depth.get(node.type_id) === d) col.push(node);
            }
            if (col.length > 0) {
                col.sort((a, b) => a.name.localeCompare(b.name));
                cols.push(col);
            }
        }
        return cols;
    });

    // ── Inline raw materials per node ────────────────────────────
    function computeInlineMaterials(nodeTypeId: number): InlineMaterial[] {
        const materials: InlineMaterial[] = [];
        for (const edge of chain.edges) {
            if (edge.to_type_id !== nodeTypeId) continue;
            const source = nodeMap.get(edge.from_type_id);
            if (!source || !isRaw(source)) continue;
            if (hiddenKinds.size > 0 && hiddenKinds.has(source.kind)) continue;
            materials.push({
                type_id: source.type_id,
                name: source.name,
                kind: source.kind,
                quantity: edge.quantity,
            });
        }
        const kindOrder: Record<string, number> = {
            raw_moon: 0,
            raw_mineral: 1,
            pi: 2,
            other: 3,
        };
        materials.sort(
            (a, b) =>
                (kindOrder[a.kind] ?? 9) - (kindOrder[b.kind] ?? 9) ||
                a.name.localeCompare(b.name),
        );
        return materials;
    }

    let visibleTypeIds = $derived.by(() => {
        const ids = new Set<number>();
        for (const col of columns)
            for (const node of col) ids.add(node.type_id);
        return ids;
    });

    let inlineMaterialsMap = $derived.by(() => {
        const map = new Map<number, InlineMaterial[]>();
        for (const col of columns) {
            for (const node of col) {
                map.set(node.type_id, computeInlineMaterials(node.type_id));
            }
        }
        return map;
    });

    // Edges: only shown when a node is focused — light up the upstream
    // production tree (everything needed to build the focused node), all
    // the way down to raws. Downstream consumers are intentionally not
    // highlighted; clicking a widely-used material would otherwise flood
    // the graph.
    let visibleEdges = $derived.by(() => {
        if (focusId === null) return [];

        const out: typeof chain.edges = [];
        const seen = new Set<number>([focusId]);
        const queue = [focusId];
        while (queue.length > 0) {
            const id = queue.pop()!;
            for (const e of chain.edges) {
                if (e.to_type_id !== id) continue;
                if (!visibleTypeIds.has(e.from_type_id)) continue;
                out.push(e);
                if (!seen.has(e.from_type_id)) {
                    seen.add(e.from_type_id);
                    queue.push(e.from_type_id);
                }
            }
        }
        return out;
    });

    // ── Build SvelteFlow nodes ──────────────────────────────────
    const COL_W = 260;
    const COL_GAP = 60;
    const ROW_GAP = 0;
    const BASE_H = 78;
    const ROW_H = 16;
    const TOGGLE_H = 18;
    const STATE_H = 24;

    function estimateNodeHeight(node: ChainNode, mats: number): number {
        let h = BASE_H;
        if (mats > 0) {
            const collapsed = collapsedInputs.has(node.type_id);
            h += TOGGLE_H + (collapsed ? 0 : mats * ROW_H + 8);
        }
        if (node.has_recipe || !focalSet.has(node.type_id)) h += STATE_H;
        return h;
    }

    let flowNodes = $derived.by(() => {
        const out: Node[] = [];
        columns.forEach((col, ci) => {
            const x = ci * (COL_W + COL_GAP);
            let y = 0;
            for (const node of col) {
                const mats = inlineMaterialsMap.get(node.type_id) ?? [];
                out.push({
                    id: String(node.type_id),
                    type: "chainNode",
                    position: { x, y },
                    data: {
                        node,
                        materials: mats,
                        isFocal: focalSet.has(node.type_id),
                    },
                    draggable: false,
                    selectable: false,
                    connectable: false,
                });
                y += estimateNodeHeight(node, mats.length) + ROW_GAP;
            }
        });
        return out;
    });

    let flowEdges = $derived.by(() => {
        return visibleEdges.map(
            (e): Edge => ({
                id: `${e.from_type_id}-${e.to_type_id}`,
                source: String(e.from_type_id),
                target: String(e.to_type_id),
                type: "default",
                animated: false,
                class: "fp-flow-edge is-hot",
            }),
        );
    });

    // SvelteFlow wants bindable nodes/edges — sync from derived to writable state
    let nodes: Node[] = $state([]);
    let edges: Edge[] = $state([]);

    $effect(() => {
        nodes = flowNodes;
    });
    $effect(() => {
        edges = flowEdges;
    });

    const nodeTypes = { chainNode: ChainNodeCard };
</script>

<div class="fp-canvas-wrap">
    <SvelteFlow
        bind:nodes
        bind:edges
        {nodeTypes}
        fitView
        minZoom={0.2}
        maxZoom={3}
        proOptions={{ hideAttribution: true }}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={false}
        panOnDrag
        zoomOnScroll
    >
        <Background />
        <Controls showLock={false} />
        <Panel position="top-left">
            <button
                class="fp-collapse-btn"
                onclick={anyExpanded ? collapseAll : expandAll}
                title={anyExpanded
                    ? "Collapse all node inputs"
                    : "Expand all node inputs"}
            >
                {anyExpanded ? "Collapse inputs" : "Expand inputs"}
            </button>
        </Panel>
    </SvelteFlow>
</div>
