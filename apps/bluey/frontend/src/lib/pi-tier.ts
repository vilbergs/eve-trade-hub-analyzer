import type { MergedChain } from "./types";

// P0 = PI nodes with has_recipe=false; otherwise tier = max(PI input tier) + 1.
export function computePiTiers(chain: MergedChain): Map<number, number> {
    const tier = new Map<number, number>();
    const piNodes = chain.nodes.filter((n) => n.kind === "pi");
    for (const n of piNodes) if (!n.has_recipe) tier.set(n.type_id, 0);

    let changed = true;
    let iterations = 0;
    while (changed && iterations < 10) {
        changed = false;
        iterations++;
        for (const n of piNodes) {
            if (!n.has_recipe) continue;
            let maxInputTier = -1;
            for (const e of chain.edges) {
                if (e.to_type_id !== n.type_id) continue;
                const inputTier = tier.get(e.from_type_id);
                const inputNode = chain.nodes.find(
                    (x) => x.type_id === e.from_type_id,
                );
                if (
                    inputNode?.kind === "pi" &&
                    inputTier !== undefined &&
                    inputTier > maxInputTier
                ) {
                    maxInputTier = inputTier;
                }
            }
            if (maxInputTier >= 0) {
                const newTier = maxInputTier + 1;
                const existing = tier.get(n.type_id);
                if (existing === undefined || newTier > existing) {
                    tier.set(n.type_id, newTier);
                    changed = true;
                }
            }
        }
    }
    return tier;
}
