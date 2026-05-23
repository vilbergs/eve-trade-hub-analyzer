import type {
  ProductEntry,
  ChainResponse,
  BomResponse,
  BomLineWithPrice,
  LedgerEntry,
  MergedChain,
  ChainNode,
  ChainEdge,
} from "./types";

export async function fetchProducts(query?: string): Promise<ProductEntry[]> {
  const params = new URLSearchParams();
  if (query) params.set("q", query);
  params.set("limit", "50");
  const res = await fetch(`/api/products?${params}`);
  if (!res.ok) throw new Error(`Products fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchChain(typeId: number): Promise<ChainResponse> {
  const res = await fetch(`/api/chain/${typeId}`);
  if (!res.ok) throw new Error(`Chain fetch failed: ${res.status}`);
  return res.json();
}

export async function fetchBom(params: {
  product_type_id: number;
  runs: number;
  me_percent: number;
  built_type_ids: number[];
}): Promise<BomResponse> {
  const res = await fetch("/api/bom", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(params),
  });
  if (!res.ok) throw new Error(`BOM fetch failed: ${res.status}`);
  return res.json();
}

export function mergeChains(chains: ChainResponse[]): MergedChain {
  const nodeMap = new Map<number, ChainNode>();
  const edgeSet = new Set<string>();
  const edges: ChainEdge[] = [];
  const focalIds: number[] = [];

  for (const chain of chains) {
    focalIds.push(chain.focal_type_id);
    for (const node of chain.nodes) {
      if (!nodeMap.has(node.type_id)) {
        nodeMap.set(node.type_id, node);
      }
    }
    for (const edge of chain.edges) {
      const key = `${edge.from_type_id}-${edge.to_type_id}`;
      if (!edgeSet.has(key)) {
        edgeSet.add(key);
        edges.push(edge);
      }
    }
  }

  return {
    focal_type_ids: focalIds,
    nodes: Array.from(nodeMap.values()),
    edges,
  };
}

export async function fetchMultiBom(
  entries: LedgerEntry[],
  builtTypeIds: number[],
): Promise<BomResponse> {
  if (entries.length === 0) {
    return { buy: [], build: [], total_cost: 0 };
  }

  const results = await Promise.all(
    entries.map((entry) =>
      fetchBom({
        product_type_id: entry.type_id,
        runs: entry.runs,
        me_percent: entry.me_percent,
        built_type_ids: builtTypeIds,
      }),
    ),
  );

  // Aggregate: merge buy and build lines by type_id, summing quantities and costs
  const buyMap = new Map<number, BomLineWithPrice>();
  const buildMap = new Map<number, BomLineWithPrice>();
  let totalCost = 0;

  for (const res of results) {
    totalCost += res.total_cost;
    for (const line of res.buy) {
      const existing = buyMap.get(line.type_id);
      if (existing) {
        existing.quantity += line.quantity;
        if (existing.line_cost != null && line.line_cost != null) {
          existing.line_cost += line.line_cost;
        } else if (line.line_cost != null) {
          existing.line_cost = line.line_cost;
        }
      } else {
        buyMap.set(line.type_id, { ...line });
      }
    }
    for (const line of res.build) {
      const existing = buildMap.get(line.type_id);
      if (existing) {
        existing.quantity += line.quantity;
        if (existing.line_cost != null && line.line_cost != null) {
          existing.line_cost += line.line_cost;
        } else if (line.line_cost != null) {
          existing.line_cost = line.line_cost;
        }
      } else {
        buildMap.set(line.type_id, { ...line });
      }
    }
  }

  return {
    buy: Array.from(buyMap.values()),
    build: Array.from(buildMap.values()),
    total_cost: totalCost,
  };
}
