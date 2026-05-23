/* FORGEPATH — graph view (pan/zoom canvas).
   Layout: each column is a flexbox (auto-sized cards stacked vertically
   with a fixed gap). Cards have natural heights. After layout, we
   measure card positions and draw edges between them.
   Drag to pan, Ctrl/Cmd + scroll to zoom. */

const { useRef: gUseRef, useState: gUseState, useEffect: gUseEffect,
        useLayoutEffect: gUseLayoutEffect, useMemo: gUseMemo,
        useCallback: gUseCallback } = React;
const { NODES: GR_NODES, edges: gEdges } = window.FORGEPATH_DATA;

// Sizing knobs ------------------------------------------------------
const CARD_W       = 300;
const CARD_W_PROD  = 360;
const CARD_VGAP    = 36;     // vertical gap between cards in a column
const COL_GAP      = 220;
const PAD_X        = 70;
const PAD_Y        = 80;
// Conservative canvas dims; the inner uses flex layout so it'll size
// naturally — we only use these for the initial-view calculation.
const CANVAS_MIN_W = PAD_X*2 + CARD_W*3 + CARD_W_PROD + COL_GAP*3;

const kindClass = {
  component: "n-comp",
  t2_product: "n-t2",
  t1_item: "n-t1",
  ram: "n-ram",
  reaction: "n-react",
  pi: "n-pi",
};
function kindLabel(node){
  switch (node.kind){
    case "component":  return "COMPONENT";
    case "t2_product": return "T2 BLUEPRINT";
    case "t1_item":    return "ITEM";
    case "ram":        return "ITEM";
    case "reaction":   return "REACTION FORMULA";
    case "pi":         return "PI · P" + (node.tier ?? 1);
    default:           return (node.kind || "").toUpperCase();
  }
}

// Canvas layout: built dynamically from the focal product's input graph.
//   col 0 = reactions, P1 PI
//   col 1 = components, P2 PI, T1, R.A.M.
//   col 2 = the focal T2 product
function buildColumns(focalId){
  const cols = [[], [], []];
  const visited = new Set();

  function walk(id){
    if (visited.has(id)) return;
    visited.add(id);
    const n = GR_NODES[id]; if(!n) return;

    if (id === focalId){
      cols[2].push(id);
    } else if (n.kind === "reaction"){
      cols[0].push(id);
    } else if (n.kind === "pi" && (n.tier ?? 1) === 1){
      cols[0].push(id);
    } else if (n.kind === "pi" && (n.tier ?? 1) === 2){
      cols[1].push(id);
    } else if (n.kind === "component" || n.kind === "t1_item" || n.kind === "ram"){
      cols[1].push(id);
    } else {
      // raw mineral, raw moon, raw planet, P0 — these are inline, not on canvas
      return;
    }

    if (n.inputs){
      for (const src of Object.keys(n.inputs)) walk(src);
    }
  }
  walk(focalId);

  // Stable order: components first (then T1 → RAM → P2 in col 1); reactions first (then P1) in col 0
  const orderKey = (id) => {
    const n = GR_NODES[id];
    if (!n) return 99;
    switch (n.kind){
      case "component":  return 0;
      case "t1_item":    return 1;
      case "ram":        return 2;
      case "reaction":   return 0;
      case "pi":         return n.tier === 2 ? 3 : 5;
      case "t2_product": return 0;
      default: return 9;
    }
  };
  for (const c of cols) c.sort((a,b)=> orderKey(a) - orderKey(b) || a.localeCompare(b));
  return cols;
}

// What counts as a "raw" material — these show up inline in cards.
// P0 PI is folded in too so P1 cards list their P0 inputs without
// requiring a deeper node.
const RAW_KINDS = new Set(["raw_mineral", "raw_moon", "raw_planet"]);

function isInlineMaterial(node){
  if (!node) return false;
  if (RAW_KINDS.has(node.kind)) return true;
  if (node.kind === "pi" && (node.tier ?? 1) === 0) return true; // P0 PI inlined too
  return false;
}

function inputRows(node){
  if (!node.inputs) return [];
  return Object.entries(node.inputs).map(([id, qty]) => {
    const m = GR_NODES[id]; if (!m) return null;
    if (!isInlineMaterial(m)) return null;
    return { id, qty, name: m.name, short: m.short, group: m.group, kind: m.kind };
  }).filter(Boolean);
}

function MaterialRow({ row }){
  const cls = "mat-" + (row.kind === "raw_moon" ? "moon"
                   : row.kind === "raw_mineral" ? "min"
                   : row.kind === "raw_planet" ? "pl"
                   : row.kind === "pi" ? "pi"
                   : "other");
  return (
    <div className={`fp-recipe-row ${cls}`}>
      <span className="fp-recipe-name">{row.name}</span>
      <span className="fp-recipe-qty">×{row.qty}</span>
    </div>
  );
}

function Card({ node, width, active, onToggle, onFocus, isFocus, runs, isFocal, cardRef }){
  const rows = inputRows(node);
  const isProd = !!isFocal;
  const isExtracted = node.kind === "pi" && (node.tier ?? 1) === 0;
  return (
    <div
      ref={cardRef}
      data-node-id={node.id}
      style={{ width }}
      className={`fp-card ${kindClass[node.kind]} ${active?"is-on":"is-off"} ${isFocus?"is-focus":""}`}
      onPointerDown={(e)=>e.stopPropagation()}
      onClick={(e)=>{ if(e.target.closest('.fp-toggle')) return; onFocus(node.id); }}
    >
      <header className="fp-card-head">
        <div className="fp-card-head-row">
          <div className="fp-card-kind">{kindLabel(node)}</div>
          {!isProd && (
            <button
              className={`fp-buildbuy ${active?"is-build":"is-buy"}`}
              onClick={(e)=>{ e.stopPropagation(); onToggle(node.id); }}
              role="switch"
              aria-checked={active}
              title={active?"Switch to BUY (will appear in BOM as-is)":"Switch to BUILD (expand into raw inputs)"}
            >
              <span className="fp-buildbuy-slider" aria-hidden="true"/>
              <span className="fp-buildbuy-opt">BUILD</span>
              <span className="fp-buildbuy-opt">BUY</span>
            </button>
          )}
        </div>
        <div className="fp-card-name">{node.name}</div>
        <div className="fp-card-meta">
          {node.race && <span className="fp-card-race">{node.race}</span>}
          {isProd && <span className="fp-card-runs">×{runs.toLocaleString()} runs</span>}
        </div>
      </header>

      {isExtracted ? null : rows.length === 0 ? (
        <div className="fp-recipe-empty">
          <div className="fp-recipe-empty-mark">↰</div>
          <div className="fp-recipe-empty-text">All inputs supplied by upstream nodes</div>
        </div>
      ) : (
        <>
          <div className="fp-recipe-label">Raw inputs · per run</div>
          <div className="fp-recipe-list">
            {rows.map(r => <MaterialRow key={r.id} row={r}/>)}
          </div>
        </>
      )}

      <span className="fp-port fp-port-in"  aria-hidden="true"/>
      <span className="fp-port fp-port-out" aria-hidden="true"/>
    </div>
  );
}

// Smooth bezier between right port of A and left port of B
function edgePath(a, b){
  const x1 = a.x + a.w, y1 = a.y + a.h/2;
  const x2 = b.x,       y2 = b.y + b.h/2;
  const dx = x2 - x1;
  const pull = Math.max(60, dx * 0.45);
  return `M ${x1} ${y1} C ${x1 + pull} ${y1}, ${x2 - pull} ${y2}, ${x2} ${y2}`;
}

function Graph({ active, toggle, focusId, setFocus, runs, focalProductId }){
  // Recompute the canvas layout whenever the focal product changes.
  const COL_LAYOUT = gUseMemo(()=>buildColumns(focalProductId), [focalProductId]);
  const CANVAS_NODE_IDS = gUseMemo(()=>COL_LAYOUT.flat(), [COL_LAYOUT]);

  const edgesOnCanvas = gUseMemo(()=>{
    const ids = new Set(CANVAS_NODE_IDS);
    return gEdges().filter(e => ids.has(e.from) && ids.has(e.to));
  }, [CANVAS_NODE_IDS]);

  const focusEdges = gUseMemo(()=>{
    const s = new Set();
    edgesOnCanvas.forEach((e,i)=>{ if(e.from===focusId || e.to===focusId) s.add(i); });
    return s;
  }, [edgesOnCanvas, focusId]);

  // ── Measured positions of each card (relative to .fp-canvas-scaled) ──
  const cardRefs = gUseRef({});
  const scaledRef = gUseRef(null);
  const [positions, setPositions] = gUseState({});
  const [canvasSize, setCanvasSize] = gUseState({ w: 0, h: 0 });

  const measure = gUseCallback(()=>{
    const root = scaledRef.current; if(!root) return;
    const rootRect = root.getBoundingClientRect();
    // Account for the scale transform: divide by current scale
    const s = scale;
    const p = {};
    for (const id of CANVAS_NODE_IDS){
      const el = cardRefs.current[id]; if(!el) continue;
      const r = el.getBoundingClientRect();
      p[id] = {
        x: (r.left - rootRect.left) / s,
        y: (r.top  - rootRect.top)  / s,
        w: r.width  / s,
        h: r.height / s,
      };
    }
    setPositions(p);
    setCanvasSize({ w: root.scrollWidth, h: root.scrollHeight });
    // eslint-disable-next-line
  }, []); // closure refreshed via dep on scale below

  // The pan/zoom state -------------------------------------------------
  const viewportRef = gUseRef(null);
  const [scale, setScale] = gUseState(1);
  const dragRef = gUseRef(null);
  const didInit = gUseRef(false);

  // Re-measure whenever layout or scale changes
  gUseLayoutEffect(()=>{
    const root = scaledRef.current; if(!root) return;
    const rootRect = root.getBoundingClientRect();
    const p = {};
    for (const id of CANVAS_NODE_IDS){
      const el = cardRefs.current[id]; if(!el) continue;
      const r = el.getBoundingClientRect();
      p[id] = {
        x: (r.left - rootRect.left) / scale,
        y: (r.top  - rootRect.top)  / scale,
        w: r.width / scale,
        h: r.height / scale,
      };
    }
    setPositions(p);
    setCanvasSize({ w: root.scrollWidth / scale, h: root.scrollHeight / scale });
  }, [scale, active, focusId, runs, CANVAS_NODE_IDS]);

  // Resize observer on the scaled inner — catches font load/reflow
  gUseEffect(()=>{
    const root = scaledRef.current; if(!root) return;
    const ro = new ResizeObserver(()=>{
      const rootRect = root.getBoundingClientRect();
      const p = {};
      for (const id of CANVAS_NODE_IDS){
        const el = cardRefs.current[id]; if(!el) continue;
        const r = el.getBoundingClientRect();
        p[id] = {
          x: (r.left - rootRect.left) / scale,
          y: (r.top  - rootRect.top)  / scale,
          w: r.width / scale,
          h: r.height / scale,
        };
      }
      setPositions(p);
      setCanvasSize({ w: root.scrollWidth / scale, h: root.scrollHeight / scale });
    });
    ro.observe(root);
    return ()=>ro.disconnect();
  }, [scale]);

  // ── Initial pan to put focal product right-of-center ──
  const initView = gUseCallback(()=>{
    const v = viewportRef.current; if(!v) return;
    const r = v.getBoundingClientRect();
    const focal = positions[focalProductId];
    if (!focal || !canvasSize.w){ return; }
    const minScale = 0.55;
    const s = Math.max(minScale, Math.min(1, r.width / (canvasSize.w * 0.65)));
    setScale(s);
    requestAnimationFrame(()=>{
      const cx = (focal.x + focal.w/2) * s;
      const cy = (focal.y + focal.h/2) * s;
      v.scrollLeft = Math.max(0, cx - r.width * 0.70);
      v.scrollTop  = Math.max(0, cy - r.height / 2);
    });
  }, [canvasSize.w, positions, focalProductId]);

  // Reset init when focal changes so we re-center on the new chain
  gUseEffect(()=>{ didInit.current = false; }, [focalProductId]);

  gUseEffect(()=>{
    if (didInit.current) return;
    if (!canvasSize.w) return;
    const v = viewportRef.current; if(!v) return;
    if (v.getBoundingClientRect().width < 10) return;
    initView();
    didInit.current = true;
  }, [initView, canvasSize.w]);

  // Pan / zoom handlers ----------------------------------------------
  function onPointerDown(e){
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const v = viewportRef.current;
    dragRef.current = { startX: e.clientX, startY: e.clientY, sl0: v.scrollLeft, st0: v.scrollTop };
  }
  function onPointerMove(e){
    const d = dragRef.current; if(!d) return;
    const v = viewportRef.current; if(!v) return;
    v.scrollLeft = d.sl0 - (e.clientX - d.startX);
    v.scrollTop  = d.st0 - (e.clientY - d.startY);
  }
  function onPointerUp(e){
    dragRef.current = null;
    try { e.currentTarget.releasePointerCapture(e.pointerId); } catch(_){}
  }
  function onWheel(e){
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const v = viewportRef.current; if(!v) return;
    const r = v.getBoundingClientRect();
    const cx = e.clientX - r.left + v.scrollLeft;
    const cy = e.clientY - r.top  + v.scrollTop;
    setScale(prev => {
      const factor = Math.exp(-e.deltaY * 0.0015);
      const ns = Math.max(0.4, Math.min(2.0, prev * factor));
      const k = ns / prev;
      requestAnimationFrame(()=>{
        v.scrollLeft = cx * k - (e.clientX - r.left);
        v.scrollTop  = cy * k - (e.clientY - r.top);
      });
      return ns;
    });
  }
  function zoomBy(mult){
    setScale(prev => {
      const v = viewportRef.current;
      const r = v.getBoundingClientRect();
      const cx = r.width/2 + v.scrollLeft;
      const cy = r.height/2 + v.scrollTop;
      const ns = Math.max(0.4, Math.min(2.0, prev * mult));
      const k = ns / prev;
      requestAnimationFrame(()=>{
        v.scrollLeft = cx * k - r.width/2;
        v.scrollTop  = cy * k - r.height/2;
      });
      return ns;
    });
  }
  function fit(){
    const v = viewportRef.current; if(!v) return;
    const r = v.getBoundingClientRect();
    const margin = 24;
    const s = Math.min((r.width - margin*2)/canvasSize.w, (r.height - margin*2)/canvasSize.h, 1.0);
    setScale(s);
    requestAnimationFrame(()=>{
      v.scrollLeft = Math.max(0, (canvasSize.w * s - r.width) / 2);
      v.scrollTop  = Math.max(0, (canvasSize.h * s - r.height) / 2);
    });
  }

  return (
    <div className="fp-canvas-wrap">
      <div
        ref={viewportRef}
        className="fp-canvas-viewport"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onWheel={onWheel}
      >
        <div
          className="fp-canvas-inner"
          style={{ width: canvasSize.w * scale || "auto", height: canvasSize.h * scale || "auto" }}
        >
          <div
            ref={scaledRef}
            className="fp-canvas-scaled"
            style={{
              transform: `scale(${scale})`,
              transformOrigin: "0 0",
              padding: `${PAD_Y}px ${PAD_X}px`,
              display: "flex",
              gap: COL_GAP + "px",
              alignItems: "stretch",
              width: "max-content",
            }}
          >
            {COL_LAYOUT.map((ids, ci) => {
              const w = ids.includes(focalProductId) ? CARD_W_PROD : CARD_W;
              return (
                <div
                  key={ci}
                  className="fp-col-stack"
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: CARD_VGAP + "px",
                    justifyContent: "center",
                    minHeight: "100%",
                  }}
                >
                  {ids.map(id => {
                    const node = GR_NODES[id];
                    return (
                      <Card
                        key={id}
                        node={node}
                        width={w}
                        isFocal={id === focalProductId}
                        cardRef={(el)=>{ cardRefs.current[id] = el; }}
                        active={active.has(id)}
                        onToggle={toggle}
                        onFocus={setFocus}
                        isFocus={focusId===id}
                        runs={runs}
                      />
                    );
                  })}
                </div>
              );
            })}

            {/* Edge layer — drawn on top, covers full canvas, never hidden by cards */}
            <svg
              className="fp-edges"
              style={{
                position: "absolute",
                left: 0, top: 0,
                width: canvasSize.w,
                height: canvasSize.h,
                pointerEvents: "none",
                overflow: "visible",
              }}
              aria-hidden="true"
            >
              <defs>
                <marker id="fp-arrow" viewBox="0 0 10 10" refX="9" refY="5"
                        markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                  <path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
                </marker>
                <marker id="fp-arrow-hot" viewBox="0 0 10 10" refX="9" refY="5"
                        markerWidth="7" markerHeight="7" orient="auto-start-reverse">
                  <path d="M0,0 L10,5 L0,10 z" fill="currentColor"/>
                </marker>
              </defs>
              {edgesOnCanvas.map((e,i)=>{
                const a = positions[e.from], b = positions[e.to];
                if(!a || !b) return null;
                const highlight = focusEdges.has(i);
                if (highlight) return null;
                const dim = focusEdges.size>0;
                return (
                  <path key={i} d={edgePath(a,b)}
                        className={`fp-edge ${dim?"is-dim":""}`}
                        markerEnd="url(#fp-arrow)"/>
                );
              })}
              {edgesOnCanvas.map((e,i)=>{
                if (!focusEdges.has(i)) return null;
                const a = positions[e.from], b = positions[e.to];
                if(!a || !b) return null;
                return (
                  <path key={"h"+i} d={edgePath(a,b)}
                        className="fp-edge is-hot"
                        markerEnd="url(#fp-arrow-hot)"/>
                );
              })}
            </svg>
          </div>
        </div>

        <div className="fp-canvas-ctl">
          <button onClick={()=>zoomBy(1.2)} title="Zoom in">+</button>
          <button onClick={()=>zoomBy(1/1.2)} title="Zoom out">−</button>
          <div className="fp-canvas-ctl-zoom">{Math.round(scale*100)}%</div>
          <button onClick={fit} title="Fit to view">⤢</button>
        </div>
      </div>
    </div>
  );
}

window.FP_Graph = Graph;
