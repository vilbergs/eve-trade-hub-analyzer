import type { NodeKind } from "../lib/types";

export type NodeState = "build" | "buy" | "off";

export interface InlineMaterial {
    type_id: number;
    name: string;
    kind: NodeKind;
    quantity: number;
}

export interface GraphContext {
    readonly focusId: number | null;
    readonly collapsedInputs: Set<number>;
    readonly quantityMap: Map<number, number>;
    readonly piTierMap: Map<number, number>;
    getNodeState(typeId: number): NodeState;
    setNodeState(typeId: number, state: NodeState): void;
    setFocus(typeId: number): void;
    toggleCollapse(typeId: number): void;
}
