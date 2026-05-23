/* FORGEPATH — aggregated BOM sidebar. */

const { NODES: BOM_NODES, computeBOM: bomCompute } = window.FORGEPATH_DATA;

function fmtISK(n){
  if (n >= 1e9) return (n/1e9).toFixed(2)+"B";
  if (n >= 1e6) return (n/1e6).toFixed(2)+"M";
  if (n >= 1e3) return (n/1e3).toFixed(1)+"K";
  return Math.round(n).toLocaleString();
}

function BOM({ active, focalId, runs, mePct, groupBy }){
  const { totals, isk } = bomCompute(focalId, active, runs, mePct);
  const ids = Object.keys(totals);

  // group rows
  const rows = Object.entries(totals).map(([id,q])=>({ id, q, node: BOM_NODES[id] }))
    .filter(r=>r.node)
    .sort((a,b)=>{
      const ga = a.node.group, gb = b.node.group;
      if (ga !== gb) return ga.localeCompare(gb);
      return (b.node.isk||0)*b.q - (a.node.isk||0)*a.q;
    });

  const grouped = {};
  for (const r of rows){
    const g = r.node.group;
    grouped[g] = grouped[g] || [];
    grouped[g].push(r);
  }
  const groupOrder = ["Moon","Mineral","Planetary","T1 base","Assembly","Component","T2 Drone","Fleet doctrine","Mission boat","Battlecruiser","Pirate cruiser"];
  const groupKeys = Object.keys(grouped).sort((a,b)=>groupOrder.indexOf(a)-groupOrder.indexOf(b));

  // total volume rough estimate (m3) — illustrative
  const m3 = rows.reduce((s,r)=> s + r.q*0.01, 0);

  return (
    <aside className="fp-bom">
      <header className="fp-bom-head">
        <div className="fp-bom-eyebrow">BILL OF MATERIALS</div>
      </header>

      <div className="fp-bom-stats">
        <div className="fp-stat">
          <div className="fp-stat-k">Est. cost</div>
          <div className="fp-stat-v fp-stat-isk">{fmtISK(isk)} <span>ISK</span></div>
        </div>
        <div className="fp-stat">
          <div className="fp-stat-k">Volume</div>
          <div className="fp-stat-v">{m3.toFixed(1)} <span>m³</span></div>
        </div>
      </div>

      {ids.length === 0 && (
        <div className="fp-bom-empty">
          <div className="fp-bom-empty-glyph">⌀</div>
          <div className="fp-bom-empty-title">No nodes active</div>
          <div className="fp-bom-empty-sub">Flip ON any node in the graph to include it in the shopping list. Raw nodes contribute themselves; component nodes are expanded into their raw inputs.</div>
        </div>
      )}

      <div className="fp-bom-list">
        {groupKeys.map(g=>(
          <section className="fp-bom-group" key={g}>
            <header className="fp-bom-group-head">
              <span className="fp-bom-group-name">{g}</span>
              <span className="fp-bom-group-count">{grouped[g].length} item{grouped[g].length===1?"":"s"}</span>
            </header>
            <ul>
              {grouped[g].map(r=>{
                const cost = (r.node.isk||0) * r.q;
                return (
                  <li key={r.id} className="fp-bom-row">
                    <div className="fp-bom-row-name">
                      <span className="fp-bom-row-full">{r.node.name}</span>
                    </div>
                    <div className="fp-bom-row-qty">{r.q.toLocaleString()}</div>
                    <div className="fp-bom-row-cost">{fmtISK(cost)}</div>
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </div>

      {ids.length>0 && (
        <footer className="fp-bom-foot">
          <button className="fp-btn fp-btn-ghost">Copy multi-buy</button>
        </footer>
      )}
    </aside>
  );
}

window.FP_BOM = BOM;
