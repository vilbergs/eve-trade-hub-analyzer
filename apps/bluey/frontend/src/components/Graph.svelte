<script lang="ts">
    import type {
        MergedChain,
        ChainNode,
        ChainEdge,
        NodeKind,
        BomResponse,
        LedgerEntry,
    } from "../lib/types";
    import { tick } from "svelte";

    interface Props {
        chain: MergedChain;
        activeSet: Set<number>;
        excludedSet: Set<number>;
        setNodeState: (typeId: number, state: "build" | "buy" | "off") => void;
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

    // Track collapsed input lists per node
    let collapsedInputs: Set<number> = $state(new Set());

    function toggleCollapse(typeId: number) {
        const next = new Set(collapsedInputs);
        if (next.has(typeId)) next.delete(typeId);
        else next.add(typeId);
        collapsedInputs = next;
    }

    // Node state helper
    type NodeState = "build" | "buy" | "off";
    function getNodeState(typeId: number): NodeState {
        if (excludedSet.has(typeId)) return "off";
        if (activeSet.has(typeId)) return "build";
        return "buy";
    }

    // Build a quantity map from BOM + ledger for display on nodes
    let quantityMap = $derived.by(() => {
        const map = new Map<number, number>();
        // Focal products: show total runs from ledger
        for (const entry of ledger) {
            map.set(entry.type_id, entry.runs);
        }
        // Intermediates/components: from BOM buy + build
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

    // Set of all focal product type_ids
    let focalSet = $derived(new Set(chain.focal_type_ids));

    // Raw nodes are NOT shown as graph nodes — they appear inline inside their consumers
    // P0 PI (has_recipe=false) is raw; P1+ PI (has_recipe=true) are graph nodes
    function isRaw(node: ChainNode): boolean {
        if (node.kind === "raw_mineral" || node.kind === "raw_moon")
            return true;
        if (node.kind === "pi" && !node.has_recipe) return true;
        if (node.kind === "other" && !node.has_recipe) return true;
        return false;
    }

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

    function kindLabel(kind: NodeKind): string {
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
            case "pi":
                return "PI";
            case "raw_mineral":
                return "MINERAL";
            case "raw_moon":
                return "MOON GOO";
            default:
                return kind.toUpperCase();
        }
    }

    // Build a lookup from type_id → ChainNode
    let nodeMap = $derived(new Map(chain.nodes.map((n) => [n.type_id, n])));

    // Compute dynamic columns based on dependency depth from focal products
    let columns = $derived.by(() => {
        // 1. Collect non-raw nodes
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
            nonRawNodes.push(node);
            nonRawIds.add(node.type_id);
        }

        // 2. Build adjacency: for each node, which non-raw nodes consume it?
        //    (edges go from → to, meaning "from" is an input to "to")
        //    We want: for each node, what's the longest path to any focal product
        const childrenOf = new Map<number, number[]>(); // node -> nodes it feeds into
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

        // 3. BFS from focal products backwards to compute depth
        //    depth 0 = focal products (rightmost)
        //    depth N = N steps away from product
        const depth = new Map<number, number>();
        for (const fid of chain.focal_type_ids) {
            depth.set(fid, 0);
        }

        // Iterative: compute max distance from any focal product
        // Use reverse BFS: for each non-raw node, depth = max(depth of consumers) + 1
        // We need to process in reverse topological order
        let changed = true;
        let iterations = 0;
        while (changed && iterations < 50) {
            changed = false;
            iterations++;
            for (const node of nonRawNodes) {
                if (focalSet.has(node.type_id)) continue;
                const consumers = childrenOf.get(node.type_id);
                if (!consumers || consumers.length === 0) {
                    // No consumers among non-raw nodes — put at depth 1
                    if (!depth.has(node.type_id)) {
                        depth.set(node.type_id, 1);
                        changed = true;
                    }
                    continue;
                }
                let maxConsumerDepth = -1;
                for (const cid of consumers) {
                    const cd = depth.get(cid);
                    if (cd !== undefined && cd > maxConsumerDepth) {
                        maxConsumerDepth = cd;
                    }
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

        // Assign depth 1 to any remaining unassigned nodes
        for (const node of nonRawNodes) {
            if (!depth.has(node.type_id)) {
                depth.set(node.type_id, 1);
            }
        }

        // 4. Group into columns by depth, sorted left (highest depth) to right (0)
        const maxDepth = Math.max(...depth.values(), 0);
        const cols: ChainNode[][] = [];
        for (let d = maxDepth; d >= 0; d--) {
            const col: ChainNode[] = [];
            for (const node of nonRawNodes) {
                if (depth.get(node.type_id) === d) {
                    col.push(node);
                }
            }
            if (col.length > 0) {
                col.sort((a, b) => a.name.localeCompare(b.name));
                cols.push(col);
            }
        }

        return cols;
    });

    // Dynamic column headers based on depth
    let colHeaders = $derived.by(() => {
        const numCols = columns.length;
        return columns.map((_, i) => {
            if (i === numCols - 1)
                return { title: `Product`, sub: "T2 output" };
            const idx = String(i + 1).padStart(2, "0");
            return { title: `${idx} · Stage ${i + 1}`, sub: "" };
        });
    });

    // Set of non-raw type_ids visible on canvas
    let visibleTypeIds = $derived.by(() => {
        const ids = new Set<number>();
        for (const col of columns) {
            for (const node of col) {
                ids.add(node.type_id);
            }
        }
        return ids;
    });

    // Edges: show the full upstream chain for every node set to "build"
    let visibleEdges = $derived.by(() => {
        // Walk backwards from all build nodes to collect reachable edges
        const reachable = new Set<number>();
        const queue = [...activeSet];
        while (queue.length > 0) {
            const id = queue.pop()!;
            if (reachable.has(id)) continue;
            reachable.add(id);
            // Find all inputs to this node
            for (const e of chain.edges) {
                if (
                    e.to_type_id === id &&
                    visibleTypeIds.has(e.from_type_id) &&
                    !reachable.has(e.from_type_id)
                ) {
                    queue.push(e.from_type_id);
                }
            }
        }
        return chain.edges.filter(
            (e) =>
                reachable.has(e.from_type_id) &&
                reachable.has(e.to_type_id) &&
                visibleTypeIds.has(e.from_type_id) &&
                visibleTypeIds.has(e.to_type_id),
        );
    });

    // For each node, compute its raw material inputs (inline display)
    type InlineMaterial = {
        type_id: number;
        name: string;
        kind: NodeKind;
        quantity: number;
    };

    function getInlineMaterials(nodeTypeId: number): InlineMaterial[] {
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

    // ─── Pan/zoom state ──────────────────────────────────────────────
    let scale = $state(1);
    let tx = $state(0);
    let ty = $state(0);
    let dragStart: {
        pointerId: number;
        startX: number;
        startY: number;
        tx0: number;
        ty0: number;
        isDragging: boolean;
    } | null = $state(null);

    const DRAG_THRESHOLD = 4; // px before we start panning

    function onPointerDown(e: PointerEvent) {
        if (e.button !== 0) return;
        // Don't start drag if clicking on a node or interactive element
        const target = e.target as HTMLElement;
        if (target.closest(".fp-node, .fp-toggle, button, input")) return;
        dragStart = {
            pointerId: e.pointerId,
            startX: e.clientX,
            startY: e.clientY,
            tx0: tx,
            ty0: ty,
            isDragging: false,
        };
    }

    function onPointerMove(e: PointerEvent) {
        if (!dragStart) return;
        const dx = e.clientX - dragStart.startX;
        const dy = e.clientY - dragStart.startY;
        if (!dragStart.isDragging) {
            if (Math.abs(dx) + Math.abs(dy) < DRAG_THRESHOLD) return;
            // Passed threshold — start actual drag
            dragStart.isDragging = true;
            (e.currentTarget as HTMLElement).setPointerCapture(
                dragStart.pointerId,
            );
        }
        tx = dragStart.tx0 + dx;
        ty = dragStart.ty0 + dy;
    }

    function onPointerUp(e: PointerEvent) {
        if (dragStart?.isDragging) {
            (e.currentTarget as HTMLElement).releasePointerCapture(
                dragStart.pointerId,
            );
        }
        dragStart = null;
    }

    function onWheel(e: WheelEvent) {
        if (!e.ctrlKey && !e.metaKey) return;
        e.preventDefault();
        const factor = e.deltaY > 0 ? 0.9 : 1.1;
        scale = Math.max(0.3, Math.min(3, scale * factor));
    }

    // ─── Focus state (edge highlighting) ────────────────────────────
    let focusId: number | null = $state(null);

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

    // ─── Node position measurement for edges ────────────────────────
    let scaledEl: HTMLDivElement | undefined = $state(undefined);
    let nodeEls = new Map<number, HTMLDivElement>();
    let positions: Map<number, { x: number; y: number; w: number; h: number }> =
        $state(new Map());

    // Svelte action to register/unregister node DOM elements
    function trackNode(el: HTMLDivElement, typeId: number) {
        nodeEls.set(typeId, el);

        return {
            update(newTypeId: number) {
                nodeEls.delete(typeId);
                typeId = newTypeId;
                nodeEls.set(typeId, el);
            },
            destroy() {
                nodeEls.delete(typeId);
            },
        };
    }

    function measurePositions() {
        if (!scaledEl) return;
        const containerRect = scaledEl.getBoundingClientRect();
        const newPositions = new Map<
            number,
            { x: number; y: number; w: number; h: number }
        >();
        for (const [typeId, el] of nodeEls) {
            if (!el) continue;
            const rect = el.getBoundingClientRect();
            const x = (rect.left - containerRect.left) / scale;
            const y = (rect.top - containerRect.top) / scale;
            const w = rect.width / scale;
            const h = rect.height / scale;
            newPositions.set(typeId, { x, y, w, h });
        }
        positions = newPositions;
    }

    // Re-measure after DOM paints whenever columns or activeSet change
    $effect(() => {
        void columns;
        void activeSet;
        tick().then(() => {
            measurePositions();
        });
    });

    // ─── Edge path computation ───────────────────────────────────────
    function computeEdgePath(
        a: { x: number; y: number; w: number; h: number },
        b: { x: number; y: number; w: number; h: number },
    ): string {
        const x1 = a.x + a.w,
            y1 = a.y + a.h / 2;
        const x2 = b.x,
            y2 = b.y + b.h / 2;
        const dx = x2 - x1;
        const pull = Math.max(60, dx * 0.45);
        return `M ${x1} ${y1} C ${x1 + pull} ${y1}, ${x2 - pull} ${y2}, ${x2} ${y2}`;
    }

    // Computed edge paths
    let edgePaths = $derived.by(() => {
        const paths: {
            key: string;
            d: string;
            from: number;
            to: number;
        }[] = [];
        for (const edge of visibleEdges) {
            const a = positions.get(edge.from_type_id);
            const b = positions.get(edge.to_type_id);
            if (!a || !b) continue;
            paths.push({
                key: `${edge.from_type_id}-${edge.to_type_id}`,
                d: computeEdgePath(a, b),
                from: edge.from_type_id,
                to: edge.to_type_id,
            });
        }
        return paths;
    });

    function edgeClass(from: number, to: number): string {
        if (focusId === null) return "fp-edge";
        if (from === focusId || to === focusId) return "fp-edge is-hot";
        return "fp-edge is-dim";
    }

    function edgeMarker(from: number, to: number): string {
        if (focusId !== null && (from === focusId || to === focusId)) {
            return "url(#fp-arrow-hot)";
        }
        return "url(#fp-arrow)";
    }

    // ─── Canvas controls ─────────────────────────────────────────────
    let viewportEl: HTMLDivElement | undefined = $state(undefined);

    function zoomIn() {
        scale = Math.min(3, scale * 1.2);
    }

    function zoomOut() {
        scale = Math.max(0.3, scale / 1.2);
    }

    function fitToView() {
        if (!viewportEl || positions.size === 0) return;
        const vpRect = viewportEl.getBoundingClientRect();

        let minX = Infinity,
            minY = Infinity,
            maxX = -Infinity,
            maxY = -Infinity;
        for (const pos of positions.values()) {
            minX = Math.min(minX, pos.x);
            minY = Math.min(minY, pos.y);
            maxX = Math.max(maxX, pos.x + pos.w);
            maxY = Math.max(maxY, pos.y + pos.h);
        }

        const contentW = maxX - minX;
        const contentH = maxY - minY;
        if (contentW <= 0 || contentH <= 0) return;

        const padding = 60;
        const scaleX = (vpRect.width - padding * 2) / contentW;
        const scaleY = (vpRect.height - padding * 2) / contentH;
        const newScale = Math.max(0.3, Math.min(2, Math.min(scaleX, scaleY)));

        const centerX = (minX + maxX) / 2;
        const centerY = (minY + maxY) / 2;

        scale = newScale;
        tx = vpRect.width / 2 - centerX * newScale;
        ty = vpRect.height / 2 - centerY * newScale;
    }
</script>

<div class="fp-canvas-wrap">
    <div
        class="fp-canvas-viewport"
        bind:this={viewportEl}
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onwheel={onWheel}
        role="application"
        aria-label="Manufacturing chain graph"
    >
        <div
            class="fp-canvas-scaled"
            bind:this={scaledEl}
            style="transform: translate({tx}px, {ty}px) scale({scale}); transform-origin: 0 0;"
        >
            <!-- SVG edge overlay -->
            <svg class="fp-edges">
                <defs>
                    <marker
                        id="fp-arrow"
                        viewBox="0 0 10 10"
                        refX="9"
                        refY="5"
                        markerWidth="6"
                        markerHeight="6"
                        orient="auto-start-reverse"
                    >
                        <path d="M0,0 L10,5 L0,10 z" fill="currentColor" />
                    </marker>
                    <marker
                        id="fp-arrow-hot"
                        viewBox="0 0 10 10"
                        refX="9"
                        refY="5"
                        markerWidth="7"
                        markerHeight="7"
                        orient="auto-start-reverse"
                    >
                        <path d="M0,0 L10,5 L0,10 z" fill="currentColor" />
                    </marker>
                </defs>
                {#each edgePaths as edge (edge.key)}
                    <path
                        class={edgeClass(edge.from, edge.to)}
                        d={edge.d}
                        marker-end={edgeMarker(edge.from, edge.to)}
                    />
                {/each}
            </svg>

            <!-- Node columns -->
            <div class="fp-cols">
                {#each columns as col, ci}
                    <div class="fp-col">
                        <div class="fp-col-head">
                            <div class="fp-col-title">
                                {colHeaders[ci].title}
                            </div>
                            <div class="fp-col-sub">{colHeaders[ci].sub}</div>
                        </div>
                        <div class="fp-col-body">
                            {#each col as node (node.type_id)}
                                {@const nodeState = getNodeState(node.type_id)}
                                {@const isFocal = focalSet.has(node.type_id)}
                                {@const materials = getInlineMaterials(
                                    node.type_id,
                                )}
                                {@const isFocused = focusId === node.type_id}
                                {@const qty = quantityMap.get(node.type_id)}
                                {@const isCollapsed = collapsedInputs.has(
                                    node.type_id,
                                )}
                                <div
                                    class="fp-node {KIND_CLASS[node.kind] ||
                                        ''}"
                                    class:is-on={nodeState === "build"}
                                    class:is-off={nodeState === "buy"}
                                    class:is-excluded={nodeState === "off"}
                                    class:is-focus={isFocal}
                                    class:is-node-focused={isFocused}
                                    use:trackNode={node.type_id}
                                    onclick={() => {
                                        setFocus(node.type_id);
                                    }}
                                    role="button"
                                    tabindex="0"
                                    onkeydown={(e) => {
                                        if (e.key === "Enter") {
                                            setFocus(node.type_id);
                                        }
                                    }}
                                >
                                    <div class="fp-node-head">
                                        <div class="fp-node-kind-row">
                                            <div class="fp-node-kind">
                                                {kindLabel(node.kind)}
                                            </div>
                                            {#if qty != null && nodeState !== "off"}
                                                <div class="fp-node-qty">
                                                    ×{qty.toLocaleString()}
                                                </div>
                                            {/if}
                                        </div>
                                        <div class="fp-node-name">
                                            {node.name}
                                        </div>
                                    </div>

                                    {#if materials.length > 0}
                                        <div
                                            class="fp-recipe"
                                            class:is-collapsed={isCollapsed}
                                        >
                                            <button
                                                class="fp-recipe-toggle"
                                                onclick={(e) => {
                                                    e.stopPropagation();
                                                    toggleCollapse(
                                                        node.type_id,
                                                    );
                                                }}
                                            >
                                                <span
                                                    class="fp-recipe-chevron"
                                                    class:is-open={!isCollapsed}
                                                    >▸</span
                                                >
                                                Inputs
                                                <span class="fp-recipe-count"
                                                    >{materials.length}</span
                                                >
                                            </button>
                                            {#if !isCollapsed}
                                                <ul class="fp-recipe-list">
                                                    {#each materials as mat (mat.type_id)}
                                                        <li
                                                            class="fp-recipe-row {MAT_CLASS[
                                                                mat.kind
                                                            ] || ''}"
                                                        >
                                                            <span
                                                                class="fp-recipe-name"
                                                                >{mat.name}</span
                                                            >
                                                            <span
                                                                class="fp-recipe-qty"
                                                                >×{mat.quantity.toLocaleString()}</span
                                                            >
                                                        </li>
                                                    {/each}
                                                </ul>
                                            {/if}
                                        </div>
                                    {/if}

                                    {#if node.has_recipe || !isFocal}
                                        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                                        <div
                                            class="fp-node-state"
                                            role="group"
                                            onclick={(e) => e.stopPropagation()}
                                        >
                                            <button
                                                class="fp-state-label fp-state-label--build"
                                                class:is-current={nodeState ===
                                                    "build"}
                                                onclick={() =>
                                                    setNodeState(
                                                        node.type_id,
                                                        "build",
                                                    )}>BUILD</button
                                            >
                                            <button
                                                class="fp-state-label fp-state-label--buy"
                                                class:is-current={nodeState ===
                                                    "buy"}
                                                onclick={() =>
                                                    setNodeState(
                                                        node.type_id,
                                                        "buy",
                                                    )}>BUY</button
                                            >
                                            <button
                                                class="fp-state-label fp-state-label--off"
                                                class:is-current={nodeState ===
                                                    "off"}
                                                onclick={() =>
                                                    setNodeState(
                                                        node.type_id,
                                                        "off",
                                                    )}>OFF</button
                                            >
                                        </div>
                                    {/if}
                                </div>
                            {/each}
                        </div>
                    </div>
                {/each}
            </div>
        </div>
    </div>

    <!-- Canvas controls -->
    <div class="fp-canvas-controls">
        <button
            class="fp-canvas-ctrl"
            onclick={zoomIn}
            title="Zoom in"
            aria-label="Zoom in"
        >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path
                    d="M8 3v10M3 8h10"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                />
            </svg>
        </button>
        <button
            class="fp-canvas-ctrl"
            onclick={zoomOut}
            title="Zoom out"
            aria-label="Zoom out"
        >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path
                    d="M3 8h10"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                />
            </svg>
        </button>
        <button
            class="fp-canvas-ctrl"
            onclick={fitToView}
            title="Fit to view"
            aria-label="Fit to view"
        >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path
                    d="M2 6V3a1 1 0 011-1h3M10 2h3a1 1 0 011 1v3M14 10v3a1 1 0 01-1 1h-3M6 14H3a1 1 0 01-1-1v-3"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
            </svg>
        </button>
    </div>
</div>
