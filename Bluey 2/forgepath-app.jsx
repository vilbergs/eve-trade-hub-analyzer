/* FORGEPATH — header + app shell + Tweaks integration */

const { useState, useMemo, useEffect: useFx } = React;
const { PRODUCTS: APP_PRODUCTS, NODES: APP_NODES } = window.FORGEPATH_DATA;

function BlueprintAutocomplete({ value, onChange, options }){
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [hi, setHi] = useState(0);
  const wrapRef = useFx ? null : null; // satisfy linter, real ref below
  const rootRef = React.useRef(null);
  const inputRef = React.useRef(null);

  const selected = options.find(o => o.id === value);
  const display = open ? query : (selected?.name || "");
  const filtered = options.filter(o =>
    !open || !query.trim() || o.name.toLowerCase().includes(query.trim().toLowerCase())
  );

  // Close on outside click
  React.useEffect(()=>{
    if (!open) return;
    function onDocClick(e){
      if (!rootRef.current?.contains(e.target)) setOpen(false);
    }
    document.addEventListener('mousedown', onDocClick);
    return ()=>document.removeEventListener('mousedown', onDocClick);
  }, [open]);

  function pick(o){
    if (o.status === "draft") return;
    onChange(o.id);
    setOpen(false);
    setQuery("");
    inputRef.current?.blur();
  }
  function onKeyDown(e){
    if (e.key === "ArrowDown"){ e.preventDefault(); setHi(h => Math.min(filtered.length-1, h+1)); }
    else if (e.key === "ArrowUp"){ e.preventDefault(); setHi(h => Math.max(0, h-1)); }
    else if (e.key === "Enter"){ e.preventDefault(); if (filtered[hi]) pick(filtered[hi]); }
    else if (e.key === "Escape"){ setOpen(false); setQuery(""); inputRef.current?.blur(); }
  }

  return (
    <div className="fp-autoc" ref={rootRef}>
      <input
        ref={inputRef}
        className="fp-autoc-input"
        type="text"
        value={display}
        placeholder="Search blueprints…"
        onChange={(e)=>{ setQuery(e.target.value); setHi(0); if(!open) setOpen(true); }}
        onFocus={()=>{ setQuery(""); setOpen(true); setHi(0); }}
        onKeyDown={onKeyDown}
        autoComplete="off"
        spellCheck={false}
      />
      <span className="fp-autoc-caret" aria-hidden="true">▾</span>
      {open && filtered.length > 0 && (
        <ul className="fp-autoc-menu" role="listbox">
          {filtered.map((o,i)=>(
            <li
              key={o.id}
              role="option"
              aria-selected={i===hi}
              aria-disabled={o.status==="draft"}
              className={`fp-autoc-opt ${i===hi?"is-hi":""} ${o.id===value?"is-sel":""} ${o.status==="draft"?"is-draft":""}`}
              onMouseDown={(e)=>{ e.preventDefault(); pick(o); }}
              onMouseEnter={()=>setHi(i)}
            >
              <span className="fp-autoc-opt-name">{o.name}</span>
              <span className="fp-autoc-opt-group">{o.group}</span>
              {o.status==="draft" && <span className="fp-autoc-opt-flag">preview</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Header({ runs, setRuns, mePct, setMePct, tePct, setTePct, focusId, setFocusId, focalProductId, setFocalProductId }){
  return (
    <header className="fp-header">
      <div className="fp-brand">
        <div className="fp-brand-mark">
          <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
            <path d="M4 8 L14 3 L24 8 L24 20 L14 25 L4 20 Z" stroke="currentColor" strokeWidth="1.5"/>
            <path d="M14 3 L14 25 M4 8 L24 20 M24 8 L4 20" stroke="currentColor" strokeWidth="1" opacity=".5"/>
            <circle cx="14" cy="14" r="3" fill="currentColor"/>
          </svg>
        </div>
        <div className="fp-brand-text">
          <div className="fp-brand-name">BLUEY</div>
          <div className="fp-brand-sub">industry planner</div>
        </div>
      </div>

      <div className="fp-header-divider"/>

      <div className="fp-product-picker">
        <div className="fp-picker-eyebrow">Blueprint</div>
        <BlueprintAutocomplete
          value={focalProductId}
          onChange={setFocalProductId}
          options={APP_PRODUCTS}
        />
      </div>

      <div className="fp-header-divider"/>

      <div className="fp-controls">
        <label className="fp-ctrl">
          <span className="fp-ctrl-k">RUNS</span>
          <input
            type="number" min="1" max="9999" step="1"
            value={runs}
            onChange={(e)=>setRuns(Math.max(1, Math.min(9999, +e.target.value||1)))}
          />
        </label>
        <label className="fp-ctrl">
          <span className="fp-ctrl-k">ME</span>
          <div className="fp-ctrl-stepper">
            <button onClick={()=>setMePct(Math.max(0, mePct-1))}>−</button>
            <span className="fp-ctrl-stepper-v">{mePct}%</span>
            <button onClick={()=>setMePct(Math.min(10, mePct+1))}>+</button>
          </div>
        </label>
        <label className="fp-ctrl">
          <span className="fp-ctrl-k">TE</span>
          <div className="fp-ctrl-stepper">
            <button onClick={()=>setTePct(Math.max(0, tePct-2))}>−</button>
            <span className="fp-ctrl-stepper-v">{tePct}%</span>
            <button onClick={()=>setTePct(Math.min(20, tePct+2))}>+</button>
          </div>
        </label>
      </div>
    </header>
  );
}

// Legend strip
function Legend(){
  const items = [
    { c:"mat-comp", t:"Component" },
    { c:"mat-react", t:"Reaction" },
    { c:"mat-pi",   t:"PI" },
    { c:"mat-t1",   t:"T1 base" },
    { c:"mat-ram",  t:"R.A.M." },
    { c:"mat-moon", t:"Moon" },
    { c:"mat-min",  t:"Mineral" },
    { c:"mat-pl",   t:"Planetary" },
  ];
  return (
    <div className="fp-legend">
      {items.map(i=>(
        <span key={i.t} className="fp-legend-item">
          <span className={`fp-legend-swatch ${i.c}`}/>
          {i.t}
        </span>
      ))}
      <span className="fp-legend-sep"/>
      <span className="fp-legend-hint">Toggle a blueprint ON to <b>build</b> it (uses its raw inputs) · leave it OFF to <b>buy</b> it directly</span>
    </div>
  );
}


function App(){
  // Resolve cross-script components at render time so the order Babel
  // finishes executing each transformed script doesn't matter.
  const Graph = window.FP_Graph;
  const BOM   = window.FP_BOM;
  const { TweaksPanel, useTweaks, TweakSection, TweakRadio, TweakToggle, TweakColor } = window;

  const TWEAK_DEFAULS = /*EDITMODE-BEGIN*/{
    "density": "comfortable",
    "showLegend": true,
    "accent": "cyan",
    "edgeStyle": "bezier"
  }/*EDITMODE-END*/;
  const [t, setTweak] = useTweaks(TWEAK_DEFAULS);

  const [runs, setRuns]   = useState(10);
  const [mePct, setMePct] = useState(10);
  const [tePct, setTePct] = useState(20);
  const [focalProductId, setFocalProductId] = useState("ishtar_t2");
  const [focusId, setFocusId] = useState("ishtar_t2");

  // Default: only the focal T2 product is "BUILD"; everything else is "BUY".
  // We re-seed this when the focal product changes so the new chain has the right default.
  const [active, setActive] = useState(() => new Set([focalProductId]));
  useFx(()=>{ setActive(new Set([focalProductId])); setFocusId(focalProductId); }, [focalProductId]);

  const toggle = (id) => {
    setActive(prev => {
      const n = new Set(prev);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  };

  // Apply accent via CSS var
  useFx(()=>{
    const map = {
      cyan:   { a: "192 90% 62%", a2:"35 92% 62%" },
      amber:  { a: "32 94% 62%",  a2:"190 90% 62%" },
      green:  { a: "150 60% 58%", a2:"35 92% 62%" },
    };
    const v = map[t.accent] || map.cyan;
    document.documentElement.style.setProperty('--accent-h', v.a);
    document.documentElement.style.setProperty('--accent2-h', v.a2);
    document.documentElement.dataset.density = t.density;
    document.documentElement.dataset.edge = t.edgeStyle;
  }, [t.accent, t.density, t.edgeStyle]);

  return (
    <div className="fp-app">
      <Header
        runs={runs} setRuns={setRuns}
        mePct={mePct} setMePct={setMePct}
        tePct={tePct} setTePct={setTePct}
        focusId={focusId} setFocusId={setFocusId}
        focalProductId={focalProductId}
        setFocalProductId={setFocalProductId}
      />

      {t.showLegend && <Legend/>}

      <main className="fp-main">
        <Graph
          active={active}
          toggle={toggle}
          focusId={focusId}
          setFocus={setFocusId}
          runs={runs}
          mePct={mePct}
          focalProductId={focalProductId}
        />
        <BOM
          active={active}
          focalId={focalProductId}
          runs={runs}
          mePct={mePct}
        />
      </main>

      <footer className="fp-footer">
        <div className="fp-foot-canvas-hint">
          <span className="kbd">drag</span> to pan
          <span className="fp-foot-sep">·</span>
          <span className="kbd">ctrl + scroll</span> to zoom
          <span className="fp-foot-sep">·</span>
          <span className="kbd">click</span> a node to refocus
          <span className="fp-foot-sep">·</span>
          toggle a blueprint to add its raw materials to the BOM
        </div>
      </footer>

      <TweaksPanel title="Tweaks">
        <TweakSection title="Layout">
          <TweakRadio
            label="Density"
            value={t.density}
            onChange={(v)=>setTweak('density', v)}
            options={[{value:"comfortable",label:"Roomy"},{value:"compact",label:"Compact"}]}
          />
          <TweakToggle label="Show legend strip" value={t.showLegend} onChange={(v)=>setTweak('showLegend', v)}/>
        </TweakSection>
        <TweakSection title="Graph">
          <TweakRadio
            label="Edge style"
            value={t.edgeStyle}
            onChange={(v)=>setTweak('edgeStyle', v)}
            options={[{value:"bezier",label:"Curved"},{value:"step",label:"Stepped"}]}
          />
        </TweakSection>
        <TweakSection title="Color">
          <TweakColor
            label="Accent"
            value={t.accent}
            onChange={(v)=>setTweak('accent', v)}
            options={["cyan","amber","green"]}
          />
        </TweakSection>
      </TweaksPanel>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App/>);
