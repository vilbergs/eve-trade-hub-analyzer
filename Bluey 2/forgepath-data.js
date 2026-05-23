/* FORGEPATH — manufacturing chain data.
   Original tool; quantities are illustrative (in-game values shift). */

// Node kinds drive color + grouping
// - raw_mineral: from ore reprocessing
// - raw_moon: harvested from moons
// - raw_planet: planetary industry
// - reaction: composite moon reaction product
// - t1_item: pre-existing T1 base item
// - ram: Robotic Assembly Module
// - component: advanced racial component
// - t2_product: focal output
// - downstream: a hull or fit consuming the T2 output

const NODES = {
  // ── Raw / source materials ─────────────────────────────────────────
  tritanium:       { id:"tritanium",       kind:"raw_mineral", name:"Tritanium",           short:"TRIT", group:"Mineral",   isk:6.2 },
  pyerite:         { id:"pyerite",         kind:"raw_mineral", name:"Pyerite",             short:"PYE",  group:"Mineral",   isk:12.5 },
  mexallon:        { id:"mexallon",        kind:"raw_mineral", name:"Mexallon",            short:"MEX",  group:"Mineral",   isk:54.0 },
  nocxium:         { id:"nocxium",         kind:"raw_mineral", name:"Nocxium",             short:"NOC",  group:"Mineral",   isk:780 },
  morphite:        { id:"morphite",        kind:"raw_mineral", name:"Morphite",            short:"MOR",  group:"Mineral",   isk:7140 },
  isogen:          { id:"isogen",          kind:"raw_mineral", name:"Isogen",              short:"ISO",  group:"Mineral",   isk:108 },
  zydrine:         { id:"zydrine",         kind:"raw_mineral", name:"Zydrine",             short:"ZYD",  group:"Mineral",   isk:1280 },
  megacyte:        { id:"megacyte",        kind:"raw_mineral", name:"Megacyte",            short:"MEG",  group:"Mineral",   isk:2880 },

  cadmium:         { id:"cadmium",         kind:"raw_moon",    name:"Cadmium",             short:"Cd",   group:"Moon",      isk:6420 },
  chromium:        { id:"chromium",        kind:"raw_moon",    name:"Chromium",            short:"Cr",   group:"Moon",      isk:5180 },
  platinum:        { id:"platinum",        kind:"raw_moon",    name:"Platinum",            short:"Pt",   group:"Moon",      isk:5760 },
  caesium:         { id:"caesium",         kind:"raw_moon",    name:"Caesium",             short:"Cs",   group:"Moon",      isk:4920 },
  vanadium:        { id:"vanadium",        kind:"raw_moon",    name:"Vanadium",            short:"V",    group:"Moon",      isk:4580 },
  promethium:      { id:"promethium",      kind:"raw_moon",    name:"Promethium",          short:"Pm",   group:"Moon",      isk:8060 },
  titanium:        { id:"titanium",        kind:"raw_moon",    name:"Titanium",            short:"Ti",   group:"Moon",      isk:5340 },
  tungsten:        { id:"tungsten",        kind:"raw_moon",    name:"Tungsten",            short:"W",    group:"Moon",      isk:5900 },
  hafnium:         { id:"hafnium",         kind:"raw_moon",    name:"Hafnium",             short:"Hf",   group:"Moon",      isk:6020 },

  silicon:         { id:"silicon",         kind:"raw_planet",  name:"Silicon",             short:"SIL",  group:"Planetary", isk:920 },
  base_metals:     { id:"base_metals",     kind:"pi", tier:0,  name:"Base Metals",         short:"BM",   group:"PI · P0",   isk:380 },
  heavy_metals:    { id:"heavy_metals",    kind:"pi", tier:0,  name:"Heavy Metals",        short:"HM",   group:"PI · P0",   isk:550 },

  // ── Reactions (composite moon products) ─────────────────────────────
  crystalline_carbonide: { id:"crystalline_carbonide", kind:"reaction", name:"Crystalline Carbonide", short:"CCAR", group:"Reaction", isk:91400,
                           inputs:{ cadmium:100, caesium:100 } },
  fernite_carbide:       { id:"fernite_carbide",       kind:"reaction", name:"Fernite Carbide",       short:"FERN", group:"Reaction", isk:88200,
                           inputs:{ vanadium:100, promethium:100 } },
  fullerides:            { id:"fullerides",            kind:"reaction", name:"Fullerides",            short:"FULL", group:"Reaction", isk:94600,
                           inputs:{ platinum:100, caesium:100 } },
  hyperflurite:          { id:"hyperflurite",          kind:"reaction", name:"Hyperflurite",          short:"HYP",  group:"Reaction", isk:84700,
                           inputs:{ chromium:100, platinum:100 } },
  sylramic_fibers:       { id:"sylramic_fibers",       kind:"reaction", name:"Sylramic Fibers",       short:"SYL",  group:"Reaction", isk:96200,
                           inputs:{ hafnium:100, promethium:100 } },
  titanium_carbide:      { id:"titanium_carbide",      kind:"reaction", name:"Titanium Carbide",      short:"TC",   group:"Reaction", isk:78800,
                           inputs:{ titanium:100, tungsten:100 } },
  tungsten_carbide:      { id:"tungsten_carbide",      kind:"reaction", name:"Tungsten Carbide",      short:"WC",   group:"Reaction", isk:82400,
                           inputs:{ tungsten:100, cadmium:100 } },

  // ── PI tiers — Construction Blocks chain ────────────────────────────
  reactive_metals:    { id:"reactive_metals",    kind:"pi", tier:1, name:"Reactive Metals",    short:"RXM", group:"PI · P1", isk:740,
                        inputs:{ base_metals:40 } },
  toxic_metals:       { id:"toxic_metals",       kind:"pi", tier:1, name:"Toxic Metals",       short:"TOX", group:"PI · P1", isk:920,
                        inputs:{ heavy_metals:40 } },
  construction:       { id:"construction",       kind:"pi", tier:2, name:"Construction Blocks", short:"CBL", group:"PI · P2", isk:14800,
                        inputs:{ reactive_metals:40, toxic_metals:40 } },

  // ── Advanced components (racial) — now consume reactions + a raw input ──
  sensor_cluster:  { id:"sensor_cluster",  kind:"component",   name:"Magnetometric Sensor Cluster", short:"MSC", group:"Component", race:"Gallente", isk:62400,
                     inputs:{ fernite_carbide:1, chromium:3 } },
  microprocessor:  { id:"microprocessor",  kind:"component",   name:"Photon Microprocessor",        short:"PMP", group:"Component", race:"Gallente", isk:48900,
                     inputs:{ fullerides:1, silicon:2 } },
  armor_plate:     { id:"armor_plate",     kind:"component",   name:"Crystalline Carbonide Armor", short:"CCA", group:"Component", race:"Gallente", isk:71200,
                     inputs:{ crystalline_carbonide:1, cadmium:5 } },
  fusion_reactor:  { id:"fusion_reactor",  kind:"component",   name:"Fusion Reactor Unit",          short:"FRU", group:"Component", race:"Gallente", isk:122000,
                     inputs:{ hyperflurite:1, platinum:4 } },
  ship_bulkheads:  { id:"ship_bulkheads",  kind:"component",   name:"Gallentean Ship Bulkheads",    short:"GSB", group:"Component", race:"Gallente", isk:88000,
                     inputs:{ titanium_carbide:1, tungsten:4 } },
  antimatter_unit: { id:"antimatter_unit", kind:"component",   name:"Antimatter Reactor Unit",      short:"ARU", group:"Component", race:"Gallente", isk:134000,
                     inputs:{ tungsten_carbide:1, cadmium:3 } },
  plasma_thruster: { id:"plasma_thruster", kind:"component",   name:"Plasma Thruster",              short:"PT",  group:"Component", race:"Gallente", isk:118500,
                     inputs:{ sylramic_fibers:1, hafnium:3 } },

  // ── T1 base & support ──────────────────────────────────────────────
  hobgoblin_t1:    { id:"hobgoblin_t1",    kind:"t1_item",     name:"Hobgoblin I",         short:"HG-I", group:"T1 base",   isk:6400,
                     inputs:{ tritanium:6200, pyerite:1500, mexallon:380, isogen:75, nocxium:18, zydrine:3 } },
  vexor_t1:        { id:"vexor_t1",        kind:"t1_item",     name:"Vexor",               short:"VEX",  group:"T1 base",   isk:28400000,
                     inputs:{ tritanium:820000, pyerite:205000, mexallon:62000, isogen:14500, nocxium:3600, zydrine:820, megacyte:210 } },
  ram_robotics:    { id:"ram_robotics",    kind:"ram",         name:"R.A.M.- Robotics",    short:"RAM",  group:"Assembly",  isk:24500,
                     inputs:{ tritanium:150, pyerite:80, mexallon:20, isogen:8, morphite:1 } },
  ram_starship:    { id:"ram_starship",    kind:"ram",         name:"R.A.M.- Starship Tech", short:"RST", group:"Assembly", isk:26800,
                     inputs:{ tritanium:180, pyerite:100, mexallon:25, isogen:10, morphite:1 } },

  // ── Focal T2 products ──────────────────────────────────────────────
  hobgoblin_t2:    { id:"hobgoblin_t2",    kind:"t2_product",  name:"Hobgoblin II",        short:"HG-II", group:"T2 Drone", race:"Gallente",
                     inputs:{ hobgoblin_t1:1, ram_robotics:1, morphite:1, construction:1,
                              sensor_cluster:1, microprocessor:1, armor_plate:1 },
                     baseTime: 1500, unitVolume: 5 },

  ishtar_t2:       { id:"ishtar_t2",       kind:"t2_product",  name:"Ishtar",              short:"ISH",   group:"Heavy Assault Cruiser", race:"Gallente",
                     inputs:{ vexor_t1:1, ram_starship:75, morphite:240, construction:150,
                              sensor_cluster:45, microprocessor:135, armor_plate:140,
                              fusion_reactor:22, ship_bulkheads:140, antimatter_unit:22, plasma_thruster:25 },
                     baseTime: 18000, unitVolume: 101000 },

};

// Multi-product picker
const PRODUCTS = [
  { id:"ishtar_t2",    name:"Ishtar",             group:"Heavy Assault Cruiser", status:"ready" },
  { id:"hobgoblin_t2", name:"Hobgoblin II",       group:"Light Combat Drone",    status:"ready" },
  { id:"hammerhead_t2",name:"Hammerhead II",      group:"Medium Combat Drone",   status:"draft" },
  { id:"warrior_t2",   name:"Warrior II",         group:"Light Combat Drone",    status:"draft" },
  { id:"hawk_t2",      name:"Hawk",               group:"Assault Frigate",       status:"draft" },
  { id:"damctrl_t2",   name:"Damage Control II",  group:"Hull Mod",              status:"draft" },
];

// Graph layout: which column each node lives in (left → right)
const COLUMNS = [
  { id:"raw",        title:"01 · Raw materials",       sub:"reprocess / mine / harvest" },
  { id:"intermed",   title:"02 · Intermediate",        sub:"base + PI + R.A.M." },
  { id:"component",  title:"03 · Components",          sub:"advanced racial parts" },
  { id:"product",    title:"04 · T2 product",          sub:"focal blueprint" },
];

function nodeColumn(node){
  switch(node.kind){
    case "raw_mineral": case "raw_moon": return "raw";
    case "raw_planet": case "reaction": return "intermed";
    case "t1_item": case "ram":         return "intermed";
    case "component":                   return "component";
    case "t2_product":                  return "product";
    default: return "raw";
  }
}

// Build a flat list keyed by column
function nodesByColumn(){
  const out = Object.fromEntries(COLUMNS.map(c => [c.id, []]));
  Object.values(NODES).forEach(n => out[nodeColumn(n)].push(n));
  // Re-order each column for visual rhythm
  const order = {
    raw: ["cadmium","chromium","platinum","caesium","vanadium","promethium","morphite","isogen","zydrine","megacyte"],
    intermed: ["silicon","construction","hobgoblin_t1","ram_robotics"],
    component: ["sensor_cluster","microprocessor","armor_plate"],
    product: ["hobgoblin_t2"],
  };
  for(const c of Object.keys(out)){
    out[c].sort((a,b) => order[c].indexOf(a.id) - order[c].indexOf(b.id));
  }
  return out;
}

// Build edge list: source -> target with qty (per run)
function edges(){
  const E = [];
  for (const n of Object.values(NODES)) {
    if (!n.inputs) continue;
    for (const [src, qty] of Object.entries(n.inputs)) {
      E.push({ from: src, to: n.id, qty });
    }
  }
  return E;
}

// Compute BOM totals. Semantics:
//   • Always starts from the focal blueprint (the thing we want to produce).
//   • For each node we encounter:
//       - if it has a recipe AND it is in the active (ON) set → BUILD it: drill into its inputs.
//       - otherwise → BUY it: it appears as a line in the BOM, not its inputs.
//   • `runs` applies to the focal node; downstream multipliers cascade naturally.
function computeBOM(focalId, activeSet, runs, mePct){
  const totals = {};   // id -> qty
  const meMul = 1 - (mePct/100);

  function process(id, mult){
    const n = NODES[id]; if(!n) return;
    const willBuild = activeSet.has(id) && n.inputs;
    if (!willBuild){
      // BUY this item — contribute it to the shopping list as itself.
      totals[id] = (totals[id]||0) + mult;
      return;
    }
    // BUILD this item — expand into its recipe inputs, applying ME on the recipe.
    for (const [src, q] of Object.entries(n.inputs)){
      process(src, mult * q * meMul);
    }
  }

  process(focalId, runs);

  // round up to int (EVE quantities are integer)
  const rounded = {};
  for (const [k,v] of Object.entries(totals)) rounded[k] = Math.max(1, Math.ceil(v));
  let isk = 0;
  for (const [k,v] of Object.entries(rounded)) isk += (NODES[k]?.isk || 0) * v;
  return { totals: rounded, isk };
}

window.FORGEPATH_DATA = { NODES, PRODUCTS, COLUMNS, nodesByColumn, edges, nodeColumn, computeBOM };
