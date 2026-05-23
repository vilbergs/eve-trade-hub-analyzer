<script lang="ts">
    interface Props {
        hiddenKinds: Set<string>;
        onToggleKind: (kind: string) => void;
    }

    let { hiddenKinds, onToggleKind }: Props = $props();

    const items = [
        { cls: "mat-comp", kind: "component", text: "Component" },
        { cls: "mat-react", kind: "reaction", text: "Reaction" },
        { cls: "mat-pi", kind: "pi", text: "PI" },
        { cls: "mat-t1", kind: "t1_item", text: "T1 base" },
        { cls: "mat-ram", kind: "ram", text: "R.A.M." },
        { cls: "mat-moon", kind: "raw_moon", text: "Moon" },
        { cls: "mat-min", kind: "raw_mineral", text: "Mineral" },
    ];
</script>

<div class="fp-legend">
    {#each items as item}
        <button
            class="fp-legend-item"
            class:is-hidden={hiddenKinds.has(item.kind)}
            onclick={() => onToggleKind(item.kind)}
            title={hiddenKinds.has(item.kind)
                ? `Show ${item.text}`
                : `Hide ${item.text}`}
        >
            <span class="fp-legend-swatch {item.cls}"></span>
            {item.text}
        </button>
    {/each}
    <span class="fp-legend-sep"></span>
    <span class="fp-legend-hint">
        Click a category to hide/show it on the canvas and BOM
    </span>
</div>
