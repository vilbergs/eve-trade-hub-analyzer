export type NodeKind =
  | "raw_mineral"
  | "raw_moon"
  | "pi"
  | "reaction"
  | "component"
  | "t1_item"
  | "ram"
  | "t2_product"
  | "other";

export interface ProductEntry {
  type_id: number;
  name: string;
  group_name: string;
  category_name: string;
}

export interface ChainNode {
  type_id: number;
  name: string;
  kind: NodeKind;
  has_recipe: boolean;
  activity_id: number;
  output_quantity: number;
  time_secs: number;
}

export interface ChainEdge {
  from_type_id: number;
  to_type_id: number;
  quantity: number;
}

export interface ChainResponse {
  focal_type_id: number;
  nodes: ChainNode[];
  edges: ChainEdge[];
}

export interface BomLineWithPrice {
  type_id: number;
  quantity: number;
  is_built: boolean;
  unit_price: number | null;
  line_cost: number | null;
  name: string | null;
}

export interface BomResponse {
  buy: BomLineWithPrice[];
  build: BomLineWithPrice[];
  total_cost: number;
}

export interface LedgerEntry {
  type_id: number;
  name: string;
  group_name: string;
  category_name: string;
  runs: number;
  me_percent: number;
}

export interface MergedChain {
  focal_type_ids: number[];
  nodes: ChainNode[];
  edges: ChainEdge[];
}
