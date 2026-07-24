# Reactivity tracer literature matrix

Status: **systematic harvest, not a finished academic survey**.  
Harvested: 2026-07-24.  
Goal: ground improvements to `vue-vet-reactivity` (local effect tracing, guards, module summaries).

## How this was built

| Step | Source | Result |
| --- | --- | --- |
| Broad arXiv query set (31 queries) | export.arxiv.org API | **385** unique hits → raw [`arxiv-harvest.json`](./arxiv-harvest.json) |
| Keyword relevance filter | title+abstract scoring | **213** “relevant-ish” → [`arxiv-relevant.json`](./arxiv-relevant.json) |
| DBLP phrase / classic author queries | dblp.org API | partial (rate-limited); classics in [`dblp-classics.json`](./dblp-classics.json) |
| Semantic Scholar seed snowball | graph API | partial (HTTP 429); some refs/cites recovered in [`s2-snowball.json`](./s2-snowball.json) |
| Manual classic + engineering lines | ACM/known PDFs/docs | filled arXiv/DBLP blind spots |

**Honesty bound.** “尽量找全” here means: main research lines for *dependency-graph tracking + static analysis as applied to reactivity* are mapped, with seed → snowball → classic fill-in. It does **not** mean every PLDI/POPL/OOPSLA paper ever written on dataflow is listed. Vue-specific static analysis of `watchEffect` remains essentially empty in the literature.

Coverage estimate after this pass: **~85–90% of the design-relevant core**, **~50–60% of the broader adjacent static-analysis ocean**.

---

## Relevance legend (for vue-vet)

| Tag | Meaning for the tracer |
| --- | --- |
| **P0** | Directly shapes fact model or algorithms we should adopt/adapt |
| **P1** | Strong design influence; read before major tracer rewrites |
| **P2** | Background / formal foundations / adjacent techniques |
| **P3** | Context only; low near-term implementation impact |
| **E** | Engineering artifact (compiler, linter, runtime), not a paper |

---

## A. Reactive programming & dependency graphs

| Year | Work | Venue / link | Pri | Why it matters |
| --- | --- | --- | --- | --- |
| 2013 | Bainomugisha et al. *A Survey on Reactive Programming* | ACM CSUR; [tech report PDF](https://soft.vub.ac.be/Publications/2012/vub-soft-tr-12-13.pdf) | **P0** | Taxonomy: push/pull, lifting, glitch freedom, distribution. Vocabulary for Vue’s implicit tracking. |
| 1997– | Elliott & Hudak *Fran* / later *Yampa* | ICFP / Haskell | **P2** | Classic continuous behaviors + events. |
| 2002– | Cooper & Krishnamurthi *FrTime* | Scheme FRP | **P2** | Dynamic dataflow graph, glitch-aware evaluation. |
| 2009 | Meyerovich et al. *Flapjax* | OOPSLA | **P1** | FRP in the browser; dataflow graph + topo/priority scheduling; web DOM binding. |
| 2010/12 | Maier & Odersky *Deprecating the Observer Pattern* / Scala.React | EPFL tech report / ScalaDays | **P0** | Implicit dep tracking via reads; **level-ordered** glitch-free propagation; “now” vs tracking read. Maps to Vue `effect` vs non-tracking read. |
| 2013 | Czaplicki & Chong *Asynchronous FRP for GUIs* (Elm) | PLDI; [PDF](https://people.seas.harvard.edu/~chong/pubs/pldi13-elm.pdf) | **P1** | Async subgraphs vs primary signal graph; UI responsiveness boundaries ≈ await/nextTick. |
| 2014 | Salvaneschi et al. *REScala* | MODULARITY | **P1** | OO+FRP bridge; dynamic reactive graph. |
| 2014 | Salvaneschi et al. *Distributed REScala* | OOPSLA | **P2** | Distributed update algorithm. |
| 2014 | Margara & Salvaneschi *We Have a DREAM* | DEBS; [PDF](https://margara.faculty.polimi.it/papers/dream_debs14.pdf) | **P1** | Consistency levels: causal / glitch-free / atomic. Cross-module “what must agree”. |
| 2017 | Ritschel *A Meta Representation for Reactive Dependency Graphs* | MSc thesis; [PDF](https://www.cs.ubc.ca/~ritschel/files/masterthesis.pdf) | **P0** | First-class reactive graph IR for analysis/transform. Blueprint for upgrading `ReactivityGraph` facts. |
| 2017 | Maier et al. *Active Expressions* | arXiv:1703.10859 | **P1** | Building blocks for RP; explicit reactive expressions. |
| 2018 | Drechsler et al. *Thread-safe Reactive Programming* | PACMPL/OOPSLA; [PDF](https://programming-group.com/assets/pdf/papers/2018-Thread-safe-reactive-programming.pdf) | **P1** | Runtime model of dynamic dependency graph + consistency under concurrency. |
| 2018 | *On the Semantics of Distributed Reactive Programming: The Cost of Consistency* | IEEE TSE | **P2** | Cost model for consistency choices. |
| 2018 | *Reactive programming experience with REScala* | ‹Programming› | **P3** | Experience report. |
| 2011 | Demetrescu et al. *Reactive Imperative Programming with Dataflow Constraints* | arXiv:1104.2293 | **P0** | Imperative host + dataflow constraints — closest academic shape to Composition API. |
| 2018 | Kamina & Aotani *Harmonizing Signals and Events* | arXiv:1803.10199 | **P1** | Signal vs event dualism → ref/computed vs watch/lifecycle. |
| 2019 | Myter et al. *Distributed Reactive Programming for Reactive Distributed Systems* | arXiv:1902.00524 | **P2** | Distributed RP survey/system angle. |
| 2022 | *Topology-Level Reactivity … Flocks* | arXiv:2202.09228 | **P2** | Topology-layer vs value-layer reactivity → project graph vs reactivity graph. |
| 2022 | Oeyen et al. *Reactive Programming on the Bare Metal* (Remus) | REBLS; [TR PDF](https://cris.vub.be/ws/portalfiles/portal/93190882/vub_tr_soft_22_15.pdf) | **P1** | Formal reactive VM; compile-time topo schedule; mono vs poly deployments. |
| 2022 | Oeyen et al. *Compiler optimizations for reactive programming* | OOPSLA (cited widely) | **P1** | AOT graph opts, dirty bits, fusion. |
| 2023 | Oeyen et al. *Tackling the Awkward Squad for RP: Actor-Reactor Model* | arXiv:2306.12313 | **P1** | Actors + reactors; awkward squad (I/O, concurrency) for RP. |
| 2024 | Oeyen et al. *Reactive Programming without Functions* | arXiv:2403.02296 | **P1** | Graph/reactor semantics without FRP-function bias. |
| 2024 | *Meerkat: Distributed RP with Live Updates* | arXiv:2407.06885 | **P3** | Live update of distributed reactive graphs. |
| 2024 | Donahue *Relational Reactive Programming* | arXiv:2408.17044 | **P3** | Relational constraints for model/view sync. |
| 2025 | *Consistent Distributed RP with Retroactive Computation* | arXiv:2502.20534 | **P2** | Retroactive consistency — analogous to seed-then-retrace modules. |
| 2025 | *Signal-First Architectures* | arXiv:2506.13815 | **P2** | Frontend signal narrative; **verify citations independently** (some look weak). |
| 2014 | Mandel et al. *Reactivity of Cooperative Systems (ReactiveML)* | SAS | **P2** | Type-and-effect for reactivity properties. |
| 2005–15 | ReactiveML line (Mandel et al.) | PPDP etc. | **P2** | Synchronous cooperative RP in ML. |

### Modal FRP (type systems; less “analyze existing JS”)

| Year | Work | Link | Pri |
| --- | --- | --- | --- |
| 2019 | *Simply RaTT* | arXiv:1903.05879 | **P2** |
| 2020 | *Diamonds are not forever* (liveness) | arXiv:2003.03170 | **P2** |
| 2023 | *Asynchronous Modal FRP* | arXiv:2303.03170 | **P2** |
| 2025 | *Simple Modal Types for FRP* | arXiv:2512.09412 | **P2** |

Use for: formalizing “sync tick / after await” boundaries — not for implementing a Vue linter typechecker.

---

## B. Incremental / demand-driven computation (runtime dependency graphs)

| Year | Work | Venue / link | Pri | Why |
| --- | --- | --- | --- | --- |
| 2002– | Acar, Blelloch, Harper et al. *Self-adjusting computation* | PLDI/POPL series | **P0** | Dynamic dependency graphs + change propagation. Vue effect tracking is a specialized SAC-like mechanism. |
| 2011 | Acar et al. *A Consistent Semantics of Self-Adjusting Computation* | arXiv:1106.0478 / JFP | **P1** | Memoization + change propagation consistency. |
| 2014 | Hammer et al. *Adapton* | PLDI | **P0** | Demand-driven incremental computation; DCG; thunks/force. Best formal analogue of “only demanded reads subscribe”. |
| 2016 | Fisher et al. *miniAdapton* | arXiv:1609.05337 | **P1** | Minimal portable DCG implementation. |
| 2016 | *Typed Adapton* | CoRR | **P3** | Nominal memoization types. |
| 2016 | Harkes et al. *IceDust* | ECOOP; [PDF](https://eelcovisser.org/publications/2016/HarkesGV16.pdf) | **P0** | **Path-based abstract interpretation** → depends-on → invert → SCC/topo schedule. Direct recipe for static computed/export graphs. |
| 2017 | Harkes & Visser *IceDust 2* | ECOOP | **P1** | Bidirectional derived relations; strategy composition. |
| 2018 | Alvarez-Picallo et al. *Fixing Incremental Computation* | arXiv:1811.06069 | **P2** | Change actions; derivatives of fixpoints; Datalog. |
| 2020 | Alvarez-Picallo *Change actions* thesis | arXiv:2002.05256 | **P2** | Unified incremental derivatives. |
| 2013 | McSherry et al. *Differential Dataflow* | CIDR | **P2** | Incremental graph computations at scale. |
| 2015 | *Foundations of Differential Dataflow* | FoSSaCS | **P2** | Formal foundations. |
| 2021 | *Efficient Parallel Self-Adjusting Computation* | arXiv:2105.06712 / SPAA | **P3** | Parallel SAC. |
| 2008 | *Provenance Traces* | arXiv:0812.0564 | **P3** | Trace/provenance for incremental systems. |

---

## C. Program dependence, slicing, control dependence

| Year | Work | Venue | Pri | Why |
| --- | --- | --- | --- | --- |
| 1981 | Weiser *Program slicing* | ICSE | **P1** | Original slicing. |
| 1987 | Ferrante, Ottenstein, Warren *The Program Dependence Graph…* | TOPLAS | **P0** | PDG = data + control dependence. Foundation for “read under guard”. |
| 1988/90 | Horwitz, Reps, Binkley *Interprocedural slicing using dependence graphs* | TOPLAS | **P0** | SDG; procedure summaries for dependence. |
| 1991 | Cytron et al. *…SSA and the Control Dependence Graph* | TOPLAS | **P1** | CDG computation. |
| 1995 | Tip *A survey of program slicing techniques* | J. Prog. Lang. | **P1** | Slicing survey. |
| 2007 | Sridharan et al. *Thin slicing* | PLDI | **P1** | Producer seed statements only; reduces noise — useful for “what feeds this effect”. |
| 2012 | *A vocabulary of program slicing-based techniques* | CSUR | **P2** | Taxonomy of slicing variants. |
| 2012 | Jaffar et al. path-sensitive backward slicing | (SAS/related) | **P1** | Path feasibility in slices. |
| 201? | Cortesi/Halder *Dependence Condition Graph* | | **P0** | Dependencies labeled with conditions — closest formal cousin of `guards[]`. |
| 2017 | *Semantical Equivalence of CFG and PDG* | arXiv:1803.02976 | **P2** | When PDG preserves CFG meaning. |
| 2020 | Chalupa et al. *Fast Computation of Strong Control Dependencies* | arXiv:2011.01564 | **P0** | Correct/faster NTSCD & DOD — upgrade path for `path_guards`. |
| 2021 | *Causal Program Dependence Analysis* | arXiv:2104.09107 | **P2** | Causal strength of dependencies (dynamic). |
| 2021 | *Efficient Path-Sensitive Data-Dependence Analysis* | arXiv:2109.07923 | **P1** | Path-sensitive data dependence. |
| 2024 | *The Parallel Semantics Program Dependence Graph* | arXiv:2402.00986 | **P3** | Parallel IR dependence. |
| 2024 | *Falcon: Fused Approach to Path-Sensitive Sparse Data Dependence Analysis* | PACMPL | **P1** | Sparse + path-sensitive dependence without full path explosion. |

---

## D. Interprocedural dataflow frameworks (summaries)

| Year | Work | Venue | Pri | Why |
| --- | --- | --- | --- | --- |
| 1978 | Sharir & Pnueli functional approach | | **P1** | Function summaries as composeable transformers. |
| 1995 | Reps, Horwitz, Sagiv *IFDS* | POPL | **P0** | Graph reachability for distributive interprocedural problems. Model for composable module summaries. |
| 1996 | Sagiv, Reps, Horwitz *IDE* | | **P0** | Environments / edge functions — richer summaries than pure IFDS sets. |
| — | Bodden *Heros* / Soot IFDS | tooling | **P2** | Practical IFDS/IDE solver. |
| 2019 | Yee et al. *Precise Dataflow Analysis of Event-Driven Applications* | arXiv:1910.12935 | **P0** | IFDS→IDE transform for handler registration order; **await/event boundaries**. |
| 2015 | Madsen, Tip, Lhoták *Static Analysis of Event-Driven Node.js* | OOPSLA | **P0** | Event-based call graphs; listener sensitivity ladder. |
| 2021 | Stein, Chang, Sridharan *Demanded Abstract Interpretation* | arXiv:2104.01270 | **P1** | Demand-driven + interactive analyzers — rules query tracer on demand. |
| 2021 | *IFDS Taint Analysis with Access Paths* | arXiv:2103.16240 | **P2** | Access-path facts (property-sensitive). |
| 2023 | *Parameterized Algorithms for Scalable Interprocedural Data-flow Analysis* | arXiv:2309.11298 | **P3** | Scaling theory. |
| 2024 | *Falcon* (also under C) | PACMPL | **P1** | Sparse path-sensitive dependence. |

---

## E. JavaScript static analysis (call graphs, types, taint)

| Year | Work | Venue / link | Pri | Why |
| --- | --- | --- | --- | --- |
| 2009 | Jensen, Møller, Thiemann *Type Analysis for JavaScript* (TAJS) | SAS | **P1** | Gold-standard JS abstract interpretation line. |
| 2013 | Keil & Thiemann *Type-based Dependency Analysis for JavaScript* | arXiv:1305.6721 | **P0** | Dependency as type abstraction / noninterference. Lattice for reactive kinds. |
| 2014 | *JSAI: … Static Analyzer for JavaScript* | arXiv:1403.3996 | **P1** | Configurable JS abstract interpreter design. |
| 2015 | Madsen et al. event-driven Node (see D) | OOPSLA | **P0** | |
| 2019 | *Automatically Tracing Imprecision Causes in JS Static Analysis* | arXiv:1909.12795 | **P2** | Diagnosing analysis imprecision. |
| 2023 | *Bimodal Taint Analysis* | arXiv:2301.10545 | **P2** | When pure static is not enough; conventions matter. |
| 2024 | Antal et al. *Static JavaScript Call Graphs: A Comparative Study* | arXiv:2405.07206 | **P1** | Call-graph quality bounds composable resolution. |
| 2022 | *Automatic Root Cause Quantification for Missing Edges in JS Call Graphs* | arXiv:2205.06780 | **P2** | Missing call-graph edges. |
| 2026 | *Change Impact Recommendation for JavaScript* | arXiv:2606.21187 | **P2** | History + runtime impact for JS changes. |
| — | WALA JS, CodeQL JS dataflow, Joern/CPG | tools | **P2** | Industrial dataflow stacks. |

---

## F. Code property graphs & multi-edge IR

| Year | Work | Venue | Pri | Why |
| --- | --- | --- | --- | --- |
| 2014 | Yamaguchi et al. *Modeling and Discovering Vulnerabilities with Code Property Graphs* | IEEE S&P | **P1** | CPG = AST+CFG+PDG; multi-edge queries. Pattern for a Reactivity Property Graph. |
| 2022 | *Representing LLVM-IR in a Code Property Graph* | arXiv:2211.05627 | **P2** | CPG beyond source languages. |
| 2026 | *AgentFlow: Building Agent Dependency Graphs…* | arXiv:2607.01640 | **P0** | **Framework-induced** dependency edges — method twin of Vue API semantics. |

---

## G. Sparse value-flow / pointer analysis (scalability substrate)

| Year | Work | Link | Pri | Why |
| --- | --- | --- | --- | --- |
| 2011 | *Lazy Pointer Analysis* | arXiv:1112.5000 | **P2** | Lazy/demand pointer analysis. |
| 2017 | *Demand-Driven Pointer Analysis with Strong Updates via Value-Flow Refinement* | arXiv:1701.05650 | **P1** | Demand + strong updates; SVF-adjacent. |
| 2019 | *Conquering Extensional Scalability for Value-Flow Analysis* | arXiv:1912.06878 | **P2** | Scaling value-flow. |
| 2021 | Shi et al. *Fusion* (path-sensitive sparse analysis) | PLDI | **P1** | Path sensitivity without enumerating all path conditions. |
| 2024 | *Anchor: Fast and Precise Value-flow Analysis for Containers* | TOSEM | **P2** | Container-sensitive value flow. |
| 2025 | *Desyan: Seamless Value-Flow and Symbolic Analysis* | arXiv:2508.00508 | **P2** | Combined platform. |
| — | SVF (Sui et al.) toolchain | open source | **P2** | Industry-standard sparse value-flow. |
| — | CFL-reachability (Reps et al.) | theory | **P2** | Underpins many demand-driven analyses. |

---

## H. Frontend / engineering counterparts (not papers, but required)

| Artifact | Pri | Mapping |
| --- | --- | --- |
| Vue 3 reactivity runtime (`effect`, `track`, `trigger`, `pauseTracking`) | **E/P0** | Ground truth for what static analysis approximates. |
| Vue Vapor Mode design discussions | **E/P1** | Compile-time static analysis of template + `<script setup>`. |
| Svelte 3/4 compiler reactive `$:` analysis | **E/P0** | Static “read but not assigned” dep graph + topo order + dirty bits. |
| Svelte 5 runes (`$state`/`$derived`/`$effect`) | **E/P1** | Hybrid: compile transform + runtime signal tracking. |
| Solid.js reactivity model / docs (Carniato) | **E/P1** | Fine-grained runtime tracking reference. |
| `eslint-plugin-react-hooks` `exhaustive-deps` | **E/P0** | Static closure free-var analysis vs explicit dep array — **dual** of Vue implicit tracking. |
| React `useEffectEvent` docs | **E/P2** | Explicit non-reactive reads inside effects. |
| Angular signals docs / RFCs | **E/P2** | Signal-first mainstreaming. |
| RxJS / ReactiveX docs | **E/P3** | Observable pipelines; different model. |

---

## I. Vue-specific academic static analysis

| Finding | Status |
| --- | --- |
| Papers on Vue `watchEffect` / computed dependency static analysis | **Essentially none found** on arXiv/DBLP for this harvest |
| Closest Vue academic hit | LLM-assisted GUI testing / router PTG extraction (e.g. arXiv:2606.27665) — navigation graphs, **not** reactivity |
| Implication | Vue Vet can define the problem class; cite RP + JS analysis + engineering duals |

---

## J. Curated “must-read” stack for tracer design (ordered)

1. Bainomugisha survey (2013) — vocabulary  
2. Scala.React / Deprecating the Observer Pattern — tracking reads + levels  
3. Adapton (PLDI 2014) — demand semantics  
4. IceDust (ECOOP 2016) — static path deps + inversion  
5. Ferrante PDG + Chalupa control dependence — guards/control  
6. DCG / conditional dependence ideas — `guards` formalization  
7. IFDS/IDE + Yee 2019 + Madsen 2015 — summaries & async  
8. Type-based Dependency Analysis for JS (2013) — kind lattice  
9. AgentFlow (2026) — framework-induced graph methodology  
10. Ritschel thesis (2017) — meta graph IR  
11. Oeyen Remus + Actor-Reactor + RP without functions — schedule/graph semantics  
12. Svelte compiler analysis + React exhaustive-deps + Vue runtime — engineering ground truth  

---

## K. Mapping to `vue-vet-reactivity` capabilities

| Tracer concern | Current state | Primary literature |
| --- | --- | --- |
| Identify Vue reactive bindings | Implemented | AgentFlow (framework edges); Scala.React kinds |
| Effect body reactive reads | `watchEffect*` only | Adapton demand; Svelte read-not-write; Vue runtime |
| Conditional / guarded reads | Ancestor-span heuristic | PDG/CDG; Chalupa NTSCD; DCG |
| After-await classification | Top-level await only | Yee event-driven; Elm async FRP; Modal FRP intuition |
| Nested callbacks excluded | Hard exclude | Madsen listener sensitivity; demand under-approx policy |
| Module summaries / composables | Coarse fixed point | IFDS/IDE; IceDust inversion; Sharir–Pnueli |
| Ambiguous quiet failure | By design | Bimodal taint (conventions); high-confidence product stance |
| Graph IR shape | Facts bag | Ritschel; CPG; AgentFlow |
| Call resolution limits | Quiet | JS call graph comparative study |
| Computed chain / derived state | Limited | IceDust; signal-first; Oeyen compiler opts |

---

## L. Harvest statistics & residual gaps

### Numbers

- arXiv unique harvested: **385**
- arXiv filter score ≥ 2: **213**
- arXiv filter score ≥ 4: **~31** (many still noisy; manual triage required)
- DBLP classic queries confirmed: PDG, thin slicing, Flapjax, REScala, TAJS, Madsen Node, ReactiveML, CPG, Differential Dataflow, DREAM, Thread-safe RP, …
- Semantic Scholar snowball: **rate-limited**; recovered related nodes include Elm PLDI’13, push-pull FRP, slicing surveys, Falcon, distributed RP semantics, …

### Still under-covered (next harvest if needed)

1. Full DBLP/Scholar citation closure on Bainomugisha survey (~400+ citing works)  
2. Esterel/Lustre/SIGNAL **causality analysis** primary sources (not Lustre FS papers)  
3. Radul & Sussman *Propagators* primary tech report  
4. SuperGlue, Trellis, Cells, .NET Rx academic treatments  
5. SAS/ECOOP/REBLS full proceedings sweeps 2015–2026  
6. Chinese-language or non-English industrial reports  
7. Commercial SAST patents on reactive UI analysis  

### Known false friends / noise

- arXiv keyword hits on “reactive”, “signal”, “soot”, “Lustre” (filesystem), “Soot” (analysis vs carbon)  
- Signal-First arXiv bibliography entries that fail independent DOI checks  
- Vulnerability/CPG papers that share graph IR but not reactivity semantics  

---

## M. Suggested product reading plan (time-boxed)

| Time | Read | Outcome |
| --- | --- | --- |
| ½ day | Bainomugisha + Vue `effect` source + this matrix §K | Shared vocabulary |
| 1 day | Scala.React + Adapton abstract/intro + IceDust §dependency analysis | Demand + static path model |
| 1 day | Ferrante PDG skim + Chalupa NTSCD + current `path_guards` | Guard redesign sketch |
| ½ day | Yee 2019 + Madsen 2015 | Async boundary lattice |
| ½ day | AgentFlow + Ritschel ch. on graph meta-rep | Fact IR evolution |
| ½ day | Svelte compiler analysis notes + exhaustive-deps implementation | Engineering duals |

---

## N. Provenance of artifacts in this folder

| File | Content |
| --- | --- |
| `arxiv-harvest.json` | Raw 385-paper arXiv harvest + query hit lists |
| `arxiv-relevant.json` | Scored subset (heuristic; re-triage before citing) |
| `dblp-harvest.json` | Early DBLP batch (truncated by rate limit) |
| `dblp-classics.json` | Classic author/title DBLP confirmations |
| `s2-snowball.json` | Partial Semantic Scholar seed/cite graph |
| `reactivity-tracer-literature.md` | This matrix (human-oriented) |

When a decision lands in code, promote the durable judgment into PCR records under `.agents/docs/` (architecture / gotchas), not only this research folder.
