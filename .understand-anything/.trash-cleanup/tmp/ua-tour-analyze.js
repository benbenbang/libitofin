#!/usr/bin/env node
"use strict";
const fs = require("fs");

function main() {
  const inPath = process.argv[2];
  const outPath = process.argv[3];
  if (!inPath || !outPath) {
    console.error("usage: analyze.js <input.json> <output.json>");
    process.exit(1);
  }
  const data = JSON.parse(fs.readFileSync(inPath, "utf8"));
  const nodes = data.nodes || [];
  const edges = data.edges || [];
  const layers = data.layers || [];

  const byId = new Map();
  for (const n of nodes) byId.set(n.id, n);

  const CODE_TYPES = new Set(["file"]);
  // node summary index (all nodes)
  const nodeSummaryIndex = {};
  for (const n of nodes) {
    nodeSummaryIndex[n.id] = { name: n.name, type: n.type, summary: n.summary || "" };
  }

  // fan in / fan out across all edges, but only count edges between real nodes
  const fanIn = new Map();
  const fanOut = new Map();
  for (const n of nodes) { fanIn.set(n.id, 0); fanOut.set(n.id, 0); }
  for (const e of edges) {
    if (byId.has(e.source)) fanOut.set(e.source, fanOut.get(e.source) + 1);
    if (byId.has(e.target)) fanIn.set(e.target, fanIn.get(e.target) + 1);
  }

  const rank = (m, key) => [...m.entries()]
    .map(([id, v]) => ({ id, [key]: v, name: byId.get(id) ? byId.get(id).name : id }))
    .sort((a, b) => b[key] - a[key])
    .slice(0, 20);

  const fanInRanking = rank(fanIn, "fanIn");
  const fanOutRanking = rank(fanOut, "fanOut");

  // entry point scoring
  const ENTRY_NAMES = new Set(["index.ts","index.js","main.ts","main.js","app.ts","app.js","server.ts","server.js","mod.rs","main.go","main.py","main.rs","manage.py","app.py","wsgi.py","asgi.py","run.py","__main__.py","Application.java","Main.java","Program.cs","config.ru","index.php","App.swift","Application.kt","main.cpp","main.c","lib.rs"]);
  const foutVals = nodes.map(n => fanOut.get(n.id)).sort((a,b)=>a-b);
  const finVals = nodes.map(n => fanIn.get(n.id)).sort((a,b)=>a-b);
  const pct = (arr, p) => arr.length ? arr[Math.min(arr.length-1, Math.floor(arr.length*p))] : 0;
  const foutTop10 = pct(foutVals, 0.9);
  const finBot25 = pct(finVals, 0.25);

  const entryScores = [];
  for (const n of nodes) {
    let score = 0;
    const fp = n.filePath || "";
    const depth = fp.split("/").length;
    if (n.type === "document") {
      if (n.name === "README.md" && depth === 1) score += 5;
      else if (/\.md$/.test(n.name) && depth === 1) score += 2;
    } else if (n.type === "file") {
      if (ENTRY_NAMES.has(n.name)) score += 3;
      if (depth <= 2) score += 1;
      if (fanOut.get(n.id) >= foutTop10) score += 1;
      if (fanIn.get(n.id) <= finBot25) score += 1;
    }
    if (score > 0) entryScores.push({ id: n.id, score, name: n.name, summary: n.summary || "" });
  }
  entryScores.sort((a,b)=>b.score-a.score);
  const entryPointCandidates = entryScores.slice(0,5);

  // BFS from top code entry point following imports + calls forward
  const adj = new Map();
  for (const n of nodes) adj.set(n.id, []);
  for (const e of edges) {
    if ((e.type === "imports" || e.type === "calls") && byId.has(e.source) && byId.has(e.target)) {
      adj.get(e.source).push(e.target);
    }
  }
  // pick top code (file) entry candidate; prefer lib.rs
  let startNode = null;
  for (const c of entryScores) {
    const t = byId.get(c.id).type;
    if (t === "file") { startNode = c.id; break; }
  }
  if (!startNode && nodes.length) {
    // fallback: highest fanOut file
    const f = fanOutRanking.find(r => byId.get(r.id) && byId.get(r.id).type === "file");
    startNode = f ? f.id : nodes[0].id;
  }

  const order = [], depthMap = {};
  if (startNode) {
    const q = [startNode];
    depthMap[startNode] = 0;
    while (q.length) {
      const cur = q.shift();
      order.push(cur);
      for (const nb of (adj.get(cur) || [])) {
        if (!(nb in depthMap)) { depthMap[nb] = depthMap[cur] + 1; q.push(nb); }
      }
    }
  }
  const byDepth = {};
  for (const id of order) {
    const d = depthMap[id];
    (byDepth[d] = byDepth[d] || []).push(id);
  }

  // non-code inventory
  const mk = n => ({ id: n.id, name: n.name, type: n.type, summary: n.summary || "" });
  const nonCodeFiles = { documentation: [], infrastructure: [], data: [], config: [] };
  for (const n of nodes) {
    if (n.type === "document") nonCodeFiles.documentation.push(mk(n));
    else if (["service","pipeline","resource"].includes(n.type)) nonCodeFiles.infrastructure.push(mk(n));
    else if (["table","schema","endpoint"].includes(n.type)) nonCodeFiles.data.push(mk(n));
    else if (n.type === "config") nonCodeFiles.config.push(mk(n));
  }

  // clusters via bidirectional imports/calls
  const pairKey = (a,b) => [a,b].sort().join("||");
  const dir = new Set();
  for (const e of edges) {
    if ((e.type==="imports"||e.type==="calls") && byId.has(e.source) && byId.has(e.target)) {
      dir.add(e.source+"->"+e.target);
    }
  }
  const clusterSets = [];
  const seen = new Set();
  for (const e of edges) {
    if (!(e.type==="imports"||e.type==="calls")) continue;
    if (!byId.has(e.source)||!byId.has(e.target)) continue;
    if (dir.has(e.target+"->"+e.source)) {
      const k = pairKey(e.source,e.target);
      if (seen.has(k)) continue;
      seen.add(k);
      clusterSets.push(new Set([e.source,e.target]));
    }
  }
  // expand
  const memberConn = (set, id) => {
    let c=0;
    for (const m of set) {
      if (dir.has(id+"->"+m)||dir.has(m+"->"+id)) c++;
    }
    return c;
  };
  for (const set of clusterSets) {
    for (const n of nodes) {
      if (set.has(n.id)) continue;
      if (set.size>=5) break;
      if (memberConn(set, n.id) >= 2) set.add(n.id);
    }
  }
  const clusterEdgeCount = set => {
    let c=0;
    for (const a of set) for (const b of set) if (a!==b && dir.has(a+"->"+b)) c++;
    return c;
  };
  const clusters = clusterSets
    .map(s => ({ nodes: [...s], edgeCount: clusterEdgeCount(s) }))
    .sort((a,b)=>b.edgeCount-a.edgeCount)
    .slice(0,10);

  const result = {
    scriptCompleted: true,
    entryPointCandidates,
    fanInRanking,
    fanOutRanking,
    bfsTraversal: { startNode, order, depthMap, byDepth },
    nonCodeFiles,
    clusters,
    layers: { count: layers.length, list: layers.map(l => ({ id: l.id, name: l.name, description: l.description })) },
    nodeSummaryIndex,
    totalNodes: nodes.length,
    totalEdges: edges.length,
  };
  fs.writeFileSync(outPath, JSON.stringify(result, null, 2));
  console.log("done. start=", startNode, "bfs reached", order.length, "nodes");
}

try { main(); } catch (e) { console.error(e.stack || String(e)); process.exit(1); }
