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
  const fileNodes = data.fileNodes || [];
  const importEdges = data.importEdges || [];
  const allEdges = data.allEdges || [];

  const idToNode = {};
  fileNodes.forEach((n) => (idToNode[n.id] = n));

  // ---- common prefix of all file paths ----
  const paths = fileNodes.map((n) => n.filePath || "");
  function commonPrefixDir(paths) {
    if (paths.length === 0) return "";
    const split = paths.map((p) => p.split("/"));
    const first = split[0];
    let prefix = [];
    for (let i = 0; i < first.length - 1; i++) {
      const seg = first[i];
      if (split.every((s) => s.length > i + 1 && s[i] === seg)) prefix.push(seg);
      else break;
    }
    return prefix.length ? prefix.join("/") + "/" : "";
  }
  const prefix = commonPrefixDir(paths);

  function groupOf(fp) {
    let rest = fp;
    if (prefix && rest.startsWith(prefix)) rest = rest.slice(prefix.length);
    const parts = rest.split("/");
    if (parts.length === 1) return "root";
    return parts[0];
  }

  // ---- A. directory groups ----
  const directoryGroups = {};
  fileNodes.forEach((n) => {
    const g = groupOf(n.filePath || "");
    (directoryGroups[g] = directoryGroups[g] || []).push(n.id);
  });

  // ---- B. node type groups ----
  const nodeTypeGroups = {};
  fileNodes.forEach((n) => {
    (nodeTypeGroups[n.type] = nodeTypeGroups[n.type] || []).push(n.id);
  });

  // ---- C. fan in/out (imports only) ----
  const fanIn = {}, fanOut = {};
  fileNodes.forEach((n) => { fanIn[n.id] = 0; fanOut[n.id] = 0; });
  importEdges.forEach((e) => {
    if (fanOut[e.source] !== undefined) fanOut[e.source]++;
    if (fanIn[e.target] !== undefined) fanIn[e.target]++;
  });

  // ---- D. cross-category edges (allEdges by type pairs) ----
  const crossMap = {};
  allEdges.forEach((e) => {
    const s = idToNode[e.source], t = idToNode[e.target];
    if (!s || !t) return;
    if (s.type === t.type) return;
    const key = s.type + "|" + t.type + "|" + e.type;
    crossMap[key] = (crossMap[key] || 0) + 1;
  });
  const crossCategoryEdges = Object.entries(crossMap).map(([k, c]) => {
    const [fromType, toType, edgeType] = k.split("|");
    return { fromType, toType, edgeType, count: c };
  });

  // ---- E. inter-group import frequency ----
  const interMap = {};
  importEdges.forEach((e) => {
    const sg = groupOf((idToNode[e.source] || {}).filePath || "");
    const tg = groupOf((idToNode[e.target] || {}).filePath || "");
    if (sg === tg) return;
    const key = sg + "|" + tg;
    interMap[key] = (interMap[key] || 0) + 1;
  });
  const interGroupImports = Object.entries(interMap).map(([k, c]) => {
    const [from, to] = k.split("|");
    return { from, to, count: c };
  });

  // ---- F. intra-group density ----
  const intraGroupDensity = {};
  Object.keys(directoryGroups).forEach((g) => {
    intraGroupDensity[g] = { internalEdges: 0, totalEdges: 0, density: 0 };
  });
  importEdges.forEach((e) => {
    const sg = groupOf((idToNode[e.source] || {}).filePath || "");
    const tg = groupOf((idToNode[e.target] || {}).filePath || "");
    if (intraGroupDensity[sg]) { intraGroupDensity[sg].totalEdges++; if (sg === tg) intraGroupDensity[sg].internalEdges++; }
    if (tg !== sg && intraGroupDensity[tg]) intraGroupDensity[tg].totalEdges++;
  });
  Object.values(intraGroupDensity).forEach((v) => {
    v.density = v.totalEdges ? +(v.internalEdges / v.totalEdges).toFixed(3) : 0;
  });

  // ---- G. pattern matching ----
  const dirPatterns = [
    [["routes","api","controllers","endpoints","handlers"],"api"],
    [["services","core","lib","domain","logic"],"service"],
    [["models","db","data","persistence","repository","entities"],"data"],
    [["components","views","pages","ui","layouts","screens"],"ui"],
    [["middleware","plugins","interceptors","guards"],"middleware"],
    [["utils","helpers","common","shared","tools","utilities"],"utility"],
    [["config","constants","env","settings"],"config"],
    [["__tests__","test","tests","spec","specs"],"test"],
    [["types","interfaces","schemas","contracts","dtos"],"types"],
    [["hooks"],"hooks"],
    [["store","state","reducers","actions","slices"],"state"],
    [["assets","static","public"],"assets"],
    [["migrations"],"data"],
    [["scripts"],"infrastructure"],
    [["docs","documentation","wiki"],"documentation"],
    [[".github",".gitlab",".circleci"],"ci-cd"],
    [["math"],"service"],
    [["patterns"],"service"],
    [["distributions"],"service"],
    [["interpolations"],"service"],
  ];
  function patternForDir(name) {
    for (const [keys, label] of dirPatterns) if (keys.includes(name)) return label;
    return null;
  }
  const patternMatches = {};
  Object.keys(directoryGroups).forEach((g) => {
    const p = patternForDir(g);
    if (p) patternMatches[g] = p;
  });

  // file-level pattern helpers
  function fileLevelPattern(fp) {
    const base = fp.split("/").pop();
    if (/\.test\.|\.spec\.|_test\.|Test\.|_spec\./.test(base)) return "test";
    if (/\.d\.ts$/.test(base)) return "types";
    if (/^Dockerfile/.test(base)) return "infrastructure";
    if (/^docker-compose/.test(base)) return "infrastructure";
    if (/\.tf$|\.tfvars$/.test(base)) return "infrastructure";
    if (/Makefile/.test(base)) return "infrastructure";
    if (/\.sql$/.test(base)) return "data";
    if (/\.(graphql|gql|proto)$/.test(base)) return "types";
    if (/\.(md|rst)$/.test(base)) return "documentation";
    if (fp.includes(".github/workflows/")) return "ci-cd";
    if (/Jenkinsfile|\.gitlab-ci\.yml/.test(base)) return "ci-cd";
    if (/^Cargo\.toml$|^go\.mod$|^Gemfile$|^pom\.xml$|^build\.gradle$|^composer\.json$/.test(base)) return "config";
    return null;
  }

  // ---- H. deployment topology ----
  const infraFiles = [];
  let hasDockerfile=false,hasCompose=false,hasK8s=false,hasTerraform=false,hasCI=false;
  fileNodes.forEach((n) => {
    const fp = n.filePath || "";
    const base = fp.split("/").pop();
    if (/^Dockerfile/.test(base)) { hasDockerfile=true; infraFiles.push(fp); }
    if (/^docker-compose/.test(base)) { hasCompose=true; infraFiles.push(fp); }
    if (/\.ya?ml$/.test(base) && /(k8s|kubernetes|helm|charts)/.test(fp)) { hasK8s=true; infraFiles.push(fp); }
    if (/\.tf$|\.tfvars$/.test(base)) { hasTerraform=true; infraFiles.push(fp); }
    if (fp.includes(".github/workflows/") || /\.gitlab-ci\.yml|Jenkinsfile/.test(base)) { hasCI=true; infraFiles.push(fp); }
  });
  const deploymentTopology = { hasDockerfile, hasCompose, hasK8s, hasTerraform, hasCI, infraFiles };

  // ---- I. data pipeline ----
  const dataPipeline = { schemaFiles: [], migrationFiles: [], dataModelFiles: [], apiHandlerFiles: [] };

  // ---- J. doc coverage ----
  const docFiles = fileNodes.filter((n) => /\.(md|rst)$/.test(n.filePath || ""));
  const groupsWithDocsSet = new Set();
  docFiles.forEach((n) => groupsWithDocsSet.add(groupOf(n.filePath)));
  const totalGroups = Object.keys(directoryGroups).length;
  const undocumentedGroups = Object.keys(directoryGroups).filter((g) => !groupsWithDocsSet.has(g));
  const docCoverage = {
    groupsWithDocs: groupsWithDocsSet.size,
    totalGroups,
    coverageRatio: totalGroups ? +(groupsWithDocsSet.size / totalGroups).toFixed(2) : 0,
    undocumentedGroups,
  };

  // ---- K. dependency direction ----
  const pairDir = {};
  interGroupImports.forEach((e) => { pairDir[e.from + "|" + e.to] = e.count; });
  const seen = new Set();
  const dependencyDirection = [];
  interGroupImports.forEach((e) => {
    const a = e.from, b = e.to;
    const key = [a, b].sort().join("##");
    if (seen.has(key)) return;
    seen.add(key);
    const ab = pairDir[a + "|" + b] || 0;
    const ba = pairDir[b + "|" + a] || 0;
    if (ab >= ba) dependencyDirection.push({ dependent: a, dependsOn: b });
    else dependencyDirection.push({ dependent: b, dependsOn: a });
  });

  // ---- file stats ----
  const filesPerGroup = {};
  Object.entries(directoryGroups).forEach(([g, arr]) => (filesPerGroup[g] = arr.length));
  const nodeTypeCounts = {};
  Object.entries(nodeTypeGroups).forEach(([t, arr]) => (nodeTypeCounts[t] = arr.length));

  const result = {
    scriptCompleted: true,
    commonPrefix: prefix,
    directoryGroups,
    nodeTypeGroups,
    crossCategoryEdges,
    interGroupImports,
    intraGroupDensity,
    patternMatches,
    fileLevelPatterns: Object.fromEntries(fileNodes.map((n)=>[n.id, fileLevelPattern(n.filePath||"")]).filter(([,v])=>v)),
    deploymentTopology,
    dataPipeline,
    docCoverage,
    dependencyDirection,
    fileStats: { totalFileNodes: fileNodes.length, filesPerGroup, nodeTypeCounts },
    fileFanIn: fanIn,
    fileFanOut: fanOut,
  };
  fs.writeFileSync(outPath, JSON.stringify(result, null, 2));
}

try { main(); } catch (e) { console.error(e.stack || String(e)); process.exit(1); }
