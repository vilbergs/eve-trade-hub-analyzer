<script lang="ts">
    import type { NodeKind } from "../lib/types";

    type NodeState = "build" | "buy" | "off";

    interface Props {
        state: NodeState;
        kind?: NodeKind;
        piTier?: number;
        onSetState: (state: NodeState) => void;
    }

    let { state, kind, piTier, onSetState }: Props = $props();

    function buildLabel(kind?: NodeKind, piTier?: number): string {
        if (kind === "pi" && piTier === 0) return "EXTRACT";
        if (
            kind === "raw_mineral" ||
            kind === "raw_moon" ||
            kind === "ice_product"
        )
            return "MINE";
        return "BUILD";
    }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    class="fp-bd-state"
    role="group"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
>
    <button
        class="fp-state-label fp-state-label--build"
        class:is-current={state === "build"}
        onclick={() => onSetState("build")}>{buildLabel(kind, piTier)}</button
    >
    <button
        class="fp-state-label fp-state-label--buy"
        class:is-current={state === "buy"}
        onclick={() => onSetState("buy")}>BUY</button
    >
    <button
        class="fp-state-label fp-state-label--off"
        class:is-current={state === "off"}
        onclick={() => onSetState("off")}>OFF</button
    >
</div>
