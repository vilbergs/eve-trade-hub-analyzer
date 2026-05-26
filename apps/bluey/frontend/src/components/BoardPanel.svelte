<script lang="ts">
    import type { LedgerEntry } from "../lib/types";

    type PipelineStage =
        | "backlog"
        | "invention"
        | "reactions"
        | "components"
        | "pi"
        | "minerals"
        | "other"
        | "manufacturing";

    interface BoardState {
        [typeId: number]: PipelineStage;
    }

    interface Props {
        ledger: LedgerEntry[];
    }

    let { ledger }: Props = $props();

    const STORAGE_KEY = "bluey:board";

    // Material sub-groups that live inside the single Materials column
    const MATERIAL_STAGES: PipelineStage[] = [
        "reactions",
        "components",
        "pi",
        "minerals",
        "other",
    ];
    const MATERIAL_SET = new Set<PipelineStage>(MATERIAL_STAGES);

    interface SubGroup {
        id: PipelineStage;
        label: string;
        accent: string;
    }

    const MATERIAL_SUBGROUPS: SubGroup[] = [
        { id: "reactions", label: "REACTIONS", accent: "var(--c-react)" },
        { id: "components", label: "COMPONENTS", accent: "var(--c-comp)" },
        { id: "pi", label: "PI", accent: "var(--c-raw-pl)" },
        { id: "minerals", label: "MINERALS", accent: "var(--c-raw-min)" },
        { id: "other", label: "OTHER", accent: "var(--text-faint)" },
    ];

    interface SimpleColumn {
        type: "simple";
        id: PipelineStage;
        label: string;
        accent: string;
    }

    interface MaterialsColumn {
        type: "materials";
        id: "materials";
        label: string;
        accent: string;
    }

    type ColumnDef = SimpleColumn | MaterialsColumn;

    const COLUMNS: ColumnDef[] = [
        {
            type: "simple",
            id: "backlog",
            label: "BACKLOG",
            accent: "var(--text-faint)",
        },
        {
            type: "simple",
            id: "invention",
            label: "INVENTION",
            accent: "var(--c-raw-moon)",
        },
        {
            type: "materials",
            id: "materials",
            label: "MATERIALS",
            accent: "var(--c-raw-pl)",
        },
        {
            type: "simple",
            id: "manufacturing",
            label: "MANUFACTURING",
            accent: "var(--c-t2)",
        },
    ];

    function loadBoard(): BoardState {
        try {
            const raw = localStorage.getItem(STORAGE_KEY);
            if (raw) {
                const parsed = JSON.parse(raw);
                // Migrate old "materials" stage to "reactions"
                for (const key of Object.keys(parsed)) {
                    if (parsed[key] === "materials") parsed[key] = "reactions";
                }
                return parsed;
            }
        } catch {
            // ignore corrupt data
        }
        return {};
    }

    function saveBoard(state: BoardState) {
        try {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
        } catch {
            // quota exceeded, silently ignore
        }
    }

    let board: BoardState = $state(loadBoard());

    // Sync board with ledger: default new entries to backlog, prune removed ones
    $effect(() => {
        const ledgerIds = new Set(ledger.map((e) => e.type_id));
        let changed = false;
        const next = { ...board };

        for (const entry of ledger) {
            if (!(entry.type_id in next)) {
                next[entry.type_id] = "backlog";
                changed = true;
            }
        }

        for (const idStr of Object.keys(next)) {
            const id = Number(idStr);
            if (!ledgerIds.has(id)) {
                delete next[id];
                changed = true;
            }
        }

        if (changed) {
            board = next;
        }
    });

    // Persist on every change
    $effect(() => {
        saveBoard(board);
    });

    let ledgerMap = $derived(new Map(ledger.map((e) => [e.type_id, e])));

    // Items for a simple (non-materials) column
    function itemsForStage(stage: PipelineStage): LedgerEntry[] {
        const items: LedgerEntry[] = [];
        for (const entry of ledger) {
            if ((board[entry.type_id] ?? "backlog") === stage) {
                items.push(entry);
            }
        }
        return items;
    }

    // Total items across all material sub-groups
    let materialsTotalCount = $derived(
        ledger.filter((e) => MATERIAL_SET.has(board[e.type_id])).length,
    );

    // Drag-and-drop state
    let dragTypeId: number | null = $state(null);
    let dropTarget: PipelineStage | null = $state(null);

    function onDragStart(e: DragEvent, typeId: number) {
        dragTypeId = typeId;
        if (e.dataTransfer) {
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", String(typeId));
        }
    }

    function onDragEnd() {
        dragTypeId = null;
        dropTarget = null;
    }

    function onDragOver(e: DragEvent, stage: PipelineStage) {
        e.preventDefault();
        if (e.dataTransfer) {
            e.dataTransfer.dropEffect = "move";
        }
        dropTarget = stage;
    }

    function onDragLeave(e: DragEvent, stage: PipelineStage) {
        if (dropTarget === stage) {
            dropTarget = null;
        }
    }

    function onDrop(e: DragEvent, stage: PipelineStage) {
        e.preventDefault();
        const raw = e.dataTransfer?.getData("text/plain");
        if (raw != null) {
            const typeId = Number(raw);
            if (ledgerMap.has(typeId)) {
                board = { ...board, [typeId]: stage };
            }
        }
        dropTarget = null;
        dragTypeId = null;
    }
</script>

{#snippet card(entry: LedgerEntry)}
    <div
        class="fp-board-card"
        class:is-dragging={dragTypeId === entry.type_id}
        draggable="true"
        role="listitem"
        ondragstart={(e) => onDragStart(e, entry.type_id)}
        ondragend={onDragEnd}
    >
        <div class="fp-board-card-name">{entry.name}</div>
        <div class="fp-board-card-meta">
            <span class="fp-board-card-group">{entry.group_name}</span>
            <span class="fp-board-card-runs">×{entry.runs}</span>
        </div>
    </div>
{/snippet}

<div class="fp-board">
    {#each COLUMNS as col (col.id)}
        {#if col.type === "simple"}
            {@const items = itemsForStage(col.id)}
            <div
                class="fp-board-col"
                class:is-drop-target={dropTarget === col.id}
                style:--col-accent={col.accent}
                role="list"
                aria-label="{col.label} column"
                ondragover={(e) => onDragOver(e, col.id)}
                ondragleave={(e) => onDragLeave(e, col.id)}
                ondrop={(e) => onDrop(e, col.id)}
            >
                <header class="fp-board-col-head">
                    <span class="fp-board-col-name">{col.label}</span>
                    <span class="fp-board-col-count">{items.length}</span>
                </header>
                <div class="fp-board-col-body">
                    {#each items as entry (entry.type_id)}
                        {@render card(entry)}
                    {/each}
                </div>
            </div>
        {:else}
            <div
                class="fp-board-col fp-board-col--materials"
                style:--col-accent={col.accent}
            >
                <header class="fp-board-col-head">
                    <span class="fp-board-col-name">{col.label}</span>
                    <span class="fp-board-col-count">{materialsTotalCount}</span
                    >
                </header>
                <div class="fp-board-col-body fp-board-col-body--subs">
                    {#each MATERIAL_SUBGROUPS as sub (sub.id)}
                        {@const subItems = itemsForStage(sub.id)}
                        <div
                            class="fp-board-sub"
                            class:is-drop-target={dropTarget === sub.id}
                            style:--sub-accent={sub.accent}
                            role="list"
                            aria-label="{sub.label} sub-group"
                            ondragover={(e) => onDragOver(e, sub.id)}
                            ondragleave={(e) => onDragLeave(e, sub.id)}
                            ondrop={(e) => onDrop(e, sub.id)}
                        >
                            <div class="fp-board-sub-head">
                                <span class="fp-board-sub-name"
                                    >{sub.label}</span
                                >
                                <span class="fp-board-sub-count"
                                    >{subItems.length}</span
                                >
                            </div>
                            <div class="fp-board-sub-body">
                                {#each subItems as entry (entry.type_id)}
                                    {@render card(entry)}
                                {/each}
                            </div>
                        </div>
                    {/each}
                </div>
            </div>
        {/if}
    {/each}
</div>

<style>
    .fp-board {
        display: flex;
        flex: 1;
        gap: 12px;
        overflow-x: auto;
        padding: 8px;
        background: var(--bg);
        min-height: 0;
    }

    /* ── Column ─────────────────────────────────── */

    .fp-board-col {
        display: flex;
        flex-direction: column;
        min-width: 200px;
        flex: 1 1 200px;
        background: var(--surface-0);
        border: 1px solid var(--line);
        border-radius: var(--radius-lg);
        overflow: hidden;
        transition: border-color 0.15s ease;
    }

    .fp-board-col.is-drop-target {
        border-color: var(--col-accent, var(--text));
        box-shadow: inset 0 0 0 1px var(--col-accent, var(--text));
    }

    .fp-board-col--materials {
        flex: 1.6 1 280px;
        min-width: 260px;
    }

    .fp-board-col-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 10px 12px;
        border-bottom: 1px solid var(--line);
        user-select: none;
    }

    .fp-board-col-name {
        font-family: var(--f-mono);
        font-size: 11px;
        font-weight: 600;
        letter-spacing: 0.08em;
        color: var(--col-accent, var(--text-dim));
    }

    .fp-board-col-count {
        font-family: var(--f-mono);
        font-size: 11px;
        color: var(--text-faint);
        background: var(--surface-1);
        padding: 1px 6px;
        border-radius: var(--radius);
    }

    .fp-board-col-body {
        display: flex;
        flex-direction: column;
        gap: 6px;
        padding: 8px;
        overflow-y: auto;
        flex: 1;
        min-height: 60px;
    }

    .fp-board-col-body--subs {
        gap: 0;
        padding: 0;
    }

    /* ── Material sub-groups ────────────────────── */

    .fp-board-sub {
        display: flex;
        flex-direction: column;
        transition: background 0.15s ease;
    }

    .fp-board-sub + .fp-board-sub {
        border-top: 1px solid var(--line);
    }

    .fp-board-sub.is-drop-target {
        background: oklch(0.21 0.012 250 / 0.6);
        box-shadow: inset 0 0 0 1px var(--sub-accent, var(--text));
    }

    .fp-board-sub-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 6px 10px;
        user-select: none;
    }

    .fp-board-sub-name {
        font-family: var(--f-mono);
        font-size: 10px;
        font-weight: 600;
        letter-spacing: 0.08em;
        color: var(--sub-accent, var(--text-faint));
    }

    .fp-board-sub-count {
        font-family: var(--f-mono);
        font-size: 10px;
        color: var(--text-faint);
    }

    .fp-board-sub-body {
        display: flex;
        flex-direction: column;
        gap: 6px;
        padding: 0 8px 8px;
        min-height: 28px;
    }

    /* ── Card ───────────────────────────────────── */

    .fp-board-card {
        background: var(--surface-1);
        border: 1px solid var(--line);
        border-radius: var(--radius);
        padding: 8px 10px;
        cursor: grab;
        transition:
            background 0.12s ease,
            opacity 0.12s ease,
            box-shadow 0.12s ease;
        user-select: none;
    }

    .fp-board-card:hover {
        background: var(--surface-2);
    }

    .fp-board-card:active {
        cursor: grabbing;
    }

    .fp-board-card.is-dragging {
        opacity: 0.35;
    }

    .fp-board-card-name {
        font-family: var(--f-sans);
        font-size: 13px;
        color: var(--text);
        line-height: 1.3;
        margin-bottom: 4px;
    }

    .fp-board-card-meta {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
    }

    .fp-board-card-group {
        font-family: var(--f-sans);
        font-size: 11px;
        color: var(--text-faint);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        min-width: 0;
    }

    .fp-board-card-runs {
        font-family: var(--f-mono);
        font-size: 11px;
        color: var(--text-dim);
        flex-shrink: 0;
    }
</style>
