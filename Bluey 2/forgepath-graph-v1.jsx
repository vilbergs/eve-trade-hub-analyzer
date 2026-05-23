/* FORGEPATH — graph view.
   Horizontal column-flow graph. Each node has a toggle.
   Edges drawn as SVG bezier curves between column rows. */

const { useRef: gUseRef, useState: gUseState, useLayoutEffect: gUseLayoutEffect, useMemo: gUseMemo } = React;
const { NODES: GR_NODES, COLUMNS: GR_COLUMNS, nodesByColumn: gNodesByColumn, edges: gEdges } = window.FORGEPATH_DATA;

// Kind → palette token
const kindClass = {
  raw_mineral: "n-raw-min",
  raw_moon:    "n-raw-moon",
  raw_planet:  "n-raw-pl",
  reaction:    "n-react",
  t1_item:     "n-t1",
  ram:         "n-ram",
  component:   "n-comp",
  t2_product:  "n-t2",
  downstream:  "n-down",
};

const kindLabel = {
  raw_mineral:"MINERAL", raw_moon:"MOON·RAW", raw_planet:"PI", reaction:"REACT",
  t1_item:"T1·BASE", ram:"R.A.M.", component:"COMPONENT", t2_product:"T2 PRODUCT",
  downstream:"DOWNSTREAM",
};

function Node({ node, active, onToggle, onFocus, isFocus, qty }) {
  return (
    <div
      className={`fp-node ${kindClass[node.kind]} ${active?"is-on":"is-off"} ${isFocus?"is-focus":""}`}
      onClick={(e)=>{ if(e.target.closest('.fp-toggle')) return; onFocus(node.id); }}
    >
      <div className="fp-node-kind">{kindLabel[node.kind]}</div>
      <div className="fp-node-name">{node.name}</div>
      <div className="fp-node-meta">
        <span className="fp-node-short">{node.short}</span>
        {qty != null && <span className="fp-node-qty">×{qty.toLocaleString()}</span>}
        {node.race && <span className="fp-node-race">{node.race}</span>}
      </div>
      <button
        className={`fp-toggle ${active?"on":"off"}`}
        onClick={(e)=>{ e.stopPropagation(); onToggle(node.id); }}
        aria-pressed={active}
        title={active?"Disable in BOM":"Enable in BOM"}
      >
        <span className="fp-toggle-track"><span className="fp-toggle-thumb"/></span>
        <span className="fp-toggle-text">{active?"ON":"OFF"}</span>
      </button>
    </div>
  );
}

function Graph({ active, toggle, focusId, setFocus, runs, mePct }){
  const cols = gUseMemo(() => gNodesByColumn(), []);
  const E    = gUseMemo(() => gEdges(), []);
  const wrapRef = gUseRef(null);
  const [positions, setPositions] = gUseState({}); // id -> {cx, cy_in, cy_out}
  const [svgSize, setSvgSize]     = gUseState({w:0,h:0});

  // Determine which edges touch the focus node — they get highlighted
  const focusEdges = gUseMemo(()=>{
    const s = new Set();
    E.forEach((e,i)=>{ if(e.from===focusId || e.to===focusId) s.add(i); });
    return s;
  }, [E, focusId]);

  // Quantity displayed on each node = quantity-needed in the FOCUS node's recipe (per run)
  // or downstream qty (uses per fit) if focus is the product.
  const focusNode = GR_NODES[focusId];
  const qtyMap = gUseMemo(()=>{
    const m = {};
    if (focusNode?.inputs){
      for (const [src,q] of Object.entries(focusNode.inputs)) m[src] = q;
    }
    m[focusId] = runs;
    return m;
  }, [focusId, runs, E, focusNode]);

  // Measure node positions for edge drawing
  gUseLayoutEffect(()=>{
    const wrap = wrapRef.current; if(!wrap) return;
    function measure(){
      const wrect = wrap.getBoundingClientRect();
      const map = {};
      wrap.querySelectorAll('[data-node-id]').forEach(el=>{
        const r = el.getBoundingClientRect();
        map[el.dataset.nodeId] = {
          right: r.right - wrect.left,
          left:  r.left  - wrect.left,
          cy:    r.top + r.height/2 - wrect.top,
        };
      });
      setPositions(map);
      setSvgSize({ w: wrect.width, h: wrap.scrollHeight });
    }
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(wrap);
    window.addEventListener('resize', measure);
    return ()=>{ ro.disconnect(); window.removeEventListener('resize', measure); };
  }, [active, focusId]);

  return (
    <div className="fp-graph" ref={wrapRef}>
      {/* Edges */}
      <svg className="fp-edges" width={svgSize.w} height={svgSize.h} aria-hidden="true">
        <defs>
          <marker id="fp-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
          </marker>
        </defs>
        {E.map((e,i)=>{
          const a = positions[e.from], b = positions[e.to];
          if(!a || !b) return null;
          const x1 = a.right, y1 = a.cy;
          const x2 = b.left,  y2 = b.cy;
          const mx = (x1+x2)/2;
          const path = `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
          const highlight = focusEdges.has(i);
          const dim = focusEdges.size>0 && !highlight;
          return (
            <path
              key={i}
              d={path}
              className={`fp-edge ${highlight?"is-hot":""} ${dim?"is-dim":""}`}
              markerEnd="url(#fp-arrow)"
            />
          );
        })}
      </svg>

      {/* Columns */}
      <div className="fp-cols">
        {GR_COLUMNS.map(col=>(
          <div key={col.id} className={`fp-col fp-col-${col.id}`}>
            <header className="fp-col-head">
              <div className="fp-col-title">{col.title}</div>
              <div className="fp-col-sub">{col.sub}</div>
            </header>
            <div className="fp-col-body">
              {cols[col.id].map(n=>(
                <div key={n.id} data-node-id={n.id} className="fp-node-slot">
                  <Node
                    node={n}
                    active={active.has(n.id)}
                    onToggle={toggle}
                    onFocus={setFocus}
                    isFocus={focusId===n.id}
                    qty={qtyMap[n.id]}
                  />
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

window.FP_Graph = Graph;
