# Reactivity tracer science memo

Harvested: 2026-08-25. Graph contract **v32** (render identifier getters). v31 is watch-source peel; v30 is pause inside followed helpers; v29 is compound / update writes.
This is a ranked research record after A0–A7 were marked complete. It is **not** a new completeness axis and it does not authorize Elk/corpus KPI chasing or another `summary/mod.rs` extract.

Related: [reactivity tracer](../reactivity-tracer.md), [literature matrix](./reactivity-tracer-literature.md), [architecture](../architecture.md) (Post-#107), [gotchas](../gotchas.md), issue [#14](https://github.com/alexzhang1030/vue-vet/issues/14).

## What the four words mean here

The request was smarter, more scientific, faster, more accurate. Those are not four new epics. They collapse onto measurements the crate already owns.

| Word | Measurable thing | Not this |
| --- | --- | --- |
| Accurate | Dual-path consistency + `tracer ⊆ runtime` | Growing A1 allowlists without an `onTrack` pair |
| Scientific | Named gates, honest algebra, oracle coverage of *shipped* facts | Calling `ExportState` a lattice while merge is last-write policy |
| Fast | Fewer AST walks, narrower incremental input, session reuse | Rewriting linking as IFDS/IDE |
| Smart | Answer consumer questions from facts already on the graph | A type-and-effect system, or deeper A4 for recall |

A0–A7 complete is a valid resting point. The stop rule in the PCR still holds: next *tracer* work needs evidence (invented edge, blocked seed, dual-path, or a consumer hole on existing facts). This memo ranks the evidence that is already sitting in the tree.

Tracker [#14](https://github.com/alexzhang1030/vue-vet/issues/14) (2026-08-10) already pointed at the same near-term work without opening a feature issue: real-repo FP passes, oracle-backed long-tail seeds, extra safe-fix producers. v23–v27 were contract refinements under that rule. The next refinements should look like v27, not like a new research framework.

## What the tracer is (v27)

Static under-approx of Vue **synchronous** tracking. Runtime is the oracle, never the product engine.

```text
bindings + aliases
  → tracking / render scopes (inline body or local identifier getter)
  → reads / uncertain / writes share follow_local_callees (depth ≤ 2)
  → classify (pause / top-level await / guards)
  → sort → ReactivityGraph
  → project: export worklist → seeds → (maybe) reparse consumer
  → template / CSS v-bind / static :prop layers
  → rules / explain-scope / digest
```

v23–v26 closed inline-vs-`load()` for reads, uncertain, writes, and `assignment_only`. v27 closed the layer above that: `computed(load)` is the getter, not a missing scope. PR #185 split collectors; it did not change the approximation. PR #188 is the last fact change on `main`.

The scientific object is a **demand graph**: which reactive reads Vue would subscribe during a sync tracking flush. Adapton's "only demanded reads subscribe" is still the right analogue. IceDust-style path inversion is already what `edges` / `to_path` are. IFDS/IDE is prior art for *summaries*, not a missing solver.

## Accurate

The remaining holes are **cross-cutting dual-paths**, not a missing fourth callee enumerator. Reads / uncertain / writes already share `follow_local_callees`. `assignment_only` is the documented exception (same `local_function_id` + async skip). Identifier getters are scope discovery in `local_getter_parts`, not a second follow.

Invention is worse than a miss. Charter: missing edges stay quiet; invented *concrete* keys and invented **Unconditional** demand are bugs.

| Hole | Code | Kind | Why it matters |
| --- | --- | --- | --- |
| Helper-followed read ignores caller control flow | **Landed v28.** Follow hops record call sites; classify uses owning-function guards plus call-site proxies so `branch_hygiene` can see both-arm helper calls. | **Invent Unconditional** (fixed) | `cond ? load() : 0` is Conditional; `cond ? load() : load()` stays Unconditional. |
| Pause / await inside a followed helper | **Landed v30** for pause. Per-function pause IR + caller hops; helper-exit leak onto later sibling reads. Await-in-helper stays quiet (async helpers are unfollowed). | **Invent Unconditional** (pause fixed) | Inline `pauseTracking(); x.value` is OutsideTracking. `load()` that pauses then reads matches. Nested `pause; pause; enable` is still last-event, not a stack. |
| `+=` / `++` writes | **Landed v29.** All non-logical assignment operators plus `UpdateExpression`. Logical `&&=` / `||=` / `??=` stay quiet. | **Dual-path miss** (fixed) | `a.value += 1` / `a.value++` record writes like `=`. |
| Writes skip sync HOF / `toValue` getters | Writes treat any nested function as drop. Reads stay inside Array/String/`toValue` callbacks (`context.rs`). | Charter-quiet miss | `list.value.map(() => { t.value = 1 })` inside computed: read of `list`, no write of `t`. |
| Composable-instance writes | Writes match `reactive_bindings` only. Reads have `bag.field.value`. | Dual-path miss | `computed(() => { bag.field.value = 1 })` may miss the write that `no-side-effects-in-computed` needs. |
| `watch((ref))` / TS-wrapped bare sources | **Landed v31.** `collect_watch_source_reads` / `collect_expression_source_reads` peel before classifying. Nested arrays still do not treat inner arrows as getters. | **Dual-path miss** (fixed) | `watch((count))` / `watch(count as any)` / `watch((() => count.value))` match the unwrapped form. |
| Render identifier callbacks | **Landed v32.** `function_like_body` resolves same-file identifiers via `local_getter_parts`. Imports, methods, and async/generator stay quiet. | **Dual-path miss** (fixed) | `render: renderFn` / `setup() { return renderFn }` match inline `render() { … }`. |
| NamedApiBag member / whole-object / partial ambient | Identifier callee + object-destructure handles. `const i18n = useI18n(); i18n.t()` quiet. Co-destructure of `{ locale, t }` injects only `locale`, not `messages`. | Charter-quiet | Elk PublishWidget was the translator-only path (synthetic bag). Member form was never the evidence. |
| CSS `v-bind` completeness | Lexical ident / quoted ident in `vue_vet_vize::style`. Members, calls, arithmetic quiet. | Charter-quiet | Prevents unused-computed FP on `v-bind(color)`. Not a Vue dep-key measurement. |

**Pause model.** Vue's `pauseTracking` / `enableTracking` / `resetTracking` are a `shouldTrack` *stack* ([`vuejs/core` `effect.ts`](https://github.com/vuejs/core/blob/main/packages/reactivity/src/effect.ts)), not a depth counter. `enableTracking` forces `shouldTrack = true`. The tracer folds events to "last pause/resume before this read" (`is_after_pause_tracking_ir`). That matches the committed `pause-tracking-window` / `reset-tracking-window` cases. Nested `pause; pause; enable` is **not** a confirmed invention. Do not "fix" it to a counter. Write an oracle case first.

**Do not revive oxc_cfg for A4.** PR #181 listed it under "not adopted." Ancestor-span + `branch_hygiene` is the in-charter model. The helper-CF hole is "attach caller guards to followed reads," not "build a CDG."

## Scientific

Accuracy is a **representative CI gate**, not a sample of apps.

| Instrument | What it measures | Size |
| --- | --- | --- |
| Runtime oracle | `tracer ⊆ runtime` and pooled recall ≥99% on committed cases | **39** `expected/*.json`. ~61 nonempty `{binding,key}` rows plus 2 empty-runtime cases. `TraceConfig::empty()` (no plugins). |
| Identifier-getter oracle (v27) | `computed(load)` / `watch(load)` vs `onTrack` | 2 cases. Not `computed(() => load())`. |
| Caller-guard oracle (v28) | `computed(() => cond ? load() : 0)` vs `onTrack` | 1 case (`computed-helper-ternary`). Kind is unit-tested. |
| Helper-pause oracle (v30) | `load()` that pauses then reads vs `onTrack` | 1 case (`pause-tracking-helper`). Leak / mixed call sites are unit-tested. |
| Watch-source peel oracle (v31) | `watch((count))` vs `onTrack` | 1 case (`watch-source-parens`). TS wrappers / array peel are unit-tested. |
| Helper-follow units | Graph-vs-graph inline vs helper | `tests/follow.rs`. No `onTrack`. |
| 280 local/module corpus | Exact `expected.reads` vs **the tracer** | Self-consistency. Gotchas already forbid treating this as recall. |
| Quality precision | Exact `(rule_id, file)` TP/FP pins | 12 projects. `reactivity-rules.json` is **4 TP + 5 FP**, matching `docs/quality-baselines.md`. |

Pooled recall can hide a one-key miss in a large HOF case and punish a one-key miss in a 1-dep case (~1.75% of 57). The 99% number is a tripwire on a hand-picked JS slice. PCR already says this. Agents keep citing it as if it were app recall. Stop.

`oracle_cases_cover_known_hard_facts` requires the committed hard-fact ids, including `watch-source-reactive-deep` and `watch-source-parens`.

Shipped facts with **no** `onTrack` pair:

| Capability | Tested how | Why the harness cannot see it today |
| --- | --- | --- |
| `computed(() => load())` helper follow | Unit only | v27 oracled the identifier form. The call form is graph-vs-graph. |
| i18n ambient `t()` | `i18n.rs` + fixture bag | Oracle uses empty plugin catalog. Runtime `wrapWithDeps` is never recorded. |
| Factory / `.d.ts` bags / companion merge | `factory.rs` | Cross-file + dts. No fair local `onTrack` identity. |
| CSS `v-bind` | Vize + unused-computed fixture | Template join, not a tracking-scope collector. |
| `defineProps` destructure / `defineModels` | `local.rs` | Harness is a plain JS module. |
| JSX Render scopes | `render.rs` `graph_tsx` | No render-effect `onTrack` install. |
| Static `:prop` flow | Project + quality FP pins | PCR: not an `onTrack` pair. |

`ExportState` is a **named-export policy algebra**, not a mathematical lattice. `merge_local` is not a join: Factory beats Composable, Known beats Factory/Composable, else last write. `merge_published` Replace can overwrite a larger bag with a smaller same-class bag. Conflicting seedable classes become sticky `Ambiguous`. That is product policy and it is fine. Calling it a lattice in the PCR is the unscientific part. Keep the executable checks in `export_lattice.rs`. Rename the *prose* when someone next touches that heading.

Three different loops use the number **8** (same-file refine, name-resolve depth, external follow depth). Same digit, three meanings. Do not unify them for numerology.

## Fast

Cold cost is dominated by (1) visiting every module in phase one, (2) cloning maps on the export worklist, (3) full Oxc reparse of every module whose seed plan is non-empty, (4) budgeted external/companion I/O. Incremental reuse *inside* `ModuleTraceState` is real. The session dirty plan still hands the tracer the **full** module set.

**Single-file walks.** Each tracking scope independently walks all nodes for members, idents, ambient reads, zero-arg callees, writes, uncertain, and await/pause IR. Reads / writes / uncertain each call `follow_local_callees`, and that function re-discovers the callee set every time (`follow.rs` `local_zero_arg_callees_in_scope` iterates every node, then `scope_context` on each zero-arg call). The *function* is shared. The *index* is not. #185 made this easier to see and did not add an index.

**Phase two is all-or-nothing.** Any non-empty `ModuleSeedPlan` (including only inject or only a typed-callback slot) reparses the whole file. Empty plan reuses `local_graph`.

**Session locality.** `DirtyPlan.export_closure` is `module_summaries.clone()` (`vue_vet_session` `locality.rs`). It does not drive the tracer. `module_summaries_visited` is `graph.module_reactivity.len()`. Prove locality with `files_parsed`, `seed_plans_recomputed`, `export_resolve_ran`, `seeded_reparses`, `graph_cow_clones`. One-shot `trace_*` benches force `persist_linking_cache = false` and are mostly unseeded. They do not measure warm `ModuleTraceState`.

**Consumers do not need more AST.** They need indexes and session reuse.

| Question | Already on the graph | Current waste |
| --- | --- | --- |
| Would Vue re-run this? | `explain_tracking_scope` | Scan all modules. VS Code `--explain-scope --no-cache`. MCP opens a new `ProjectSession`. |
| Who reads `props.count`? | `edges` + `to_path` | TUI/VS Code filter label strings. |
| Scope at caret | `scope.span` covering | Linear `min_by_key`. VS Code reimplements it on `scope_details`. |
| Tracer ran? | CLI `reactivity` totals | MCP `vue_vet_scan` sets `reactivity: None`. |

Stein et al. *Demanded Abstract Interpretation* (PLDI 2021) and *Demanded Summarization* (TOPLAS 2024) are the right *vocabulary* for "query + incrementality." Vue Vet already has a demanded *product* shape (rules walk facts; hover reads a committed snapshot). It does not have a DAIG. Do not build one. Narrow the session input set and add a binding/offset index.

ECOOP 2025 *Reusing Caches and Invariants* (function-summary reuse when the analysis context is unchanged) maps to `ModuleSeedPlan` equality, which already exists. The missing piece is not a new summary IR. It is stopping the session from asking phase one to visit every attached summary on an independent leaf edit.

Historical stall still in the PCR: companion `.js` over-merge parsed `typescript.js` (~9 MB) and stalled `pixi-heatmap/docs` (~20 s). The guard is `needs_implementation_merge` on provisional halves only. Do not reopen "no finished seeds means incomplete."

Documented wall times are a 2026-07-27 maintainer spot check on `nuxt-graph` (session-analyze milliseconds, CLI process ~0.3 s). CodSpeed has names, not committed CPU times. `scan_diff_filter_nuxt_graph` measures one `filter_diff` only; analyze and cache teardown stay outside the timed closure. Do not treat that name as a scan-path regression.

## Smart

Smarter is **not** a bigger algorithm.

Willow (Wunder, Das, Gaboardi, 2026, [arXiv:2607.27074](https://arxiv.org/abs/2607.27074)) is a React-inspired core calculus with a type-and-effect system whose effects form a *temporal* dependency graph (render cascades, inter-render loops). Useful as vocabulary for "would this re-run, and when." It is a typechecker for a toy language. Vue Vet analyzes existing JS/Vue under under-approx + quiet failure. Do not import `○` modalities.

ng-reactive-lint ([arXiv:2512.00250](https://arxiv.org/abs/2512.00250)) is the closest *product* dual: framework-aware reactivity lint, evaluated on real Angular apps. Their lesson is evaluation methodology (owned corpus + offline real repos), which Vue Vet already has. Their AST anti-pattern list is not a tracer design.

AgentFlow (already in the 2026-07-24 matrix) is still the right methodology paper: framework-induced edges, not host-language dataflow. `NamedApiBag` is that idea made table-driven. Grow the catalog only with oracle or Elk-class evidence.

Vue 3.6 Vapor is compile-time template wiring. Same *problem class*, different product. Vue Vet must not become a second Vapor IR. Alien-signals is already in Vue 3.5+ ([vuejs/core#12349](https://github.com/vuejs/core/pull/12349), [#12570](https://github.com/vuejs/core/pull/12570)): push dirty flags, linked-list deps, same `onTrack` / `pauseTracking` stack. Refresh the oracle when the product Vue pin moves. Do not model the linked list.

IFDS/IDE would explode a supergraph Vue Vet does not have. A6 is a finite named-export enum + worklist + publish barrier. That is the right complexity for under-approx composable seeds. Literature §K used to say "coarse fixed point" as if IFDS were the unfinished destination. It is not.

Depth-2 helper follow is a bound, not a bug. Cross-file / args / methods stay quiet on purpose. Raise the bound only with a fixture that depth 2 misses *and* an oracle pair. StatusReactedBy was depth 1.

## Ranked next work

Stay inside the PCR stop rule. Each row says what evidence already exists and what a change must add.

### Do now if someone is already in the file (contract)

1. **Caller guards on followed reads.** Landed in v28. Fixture: `computed(() => cond ? load() : 0)` vs inline ternary. Both-arm `load()` stays Unconditional. Oracle: `computed-helper-ternary`.
2. **Pause inside a followed helper.** Landed in v30. Owning-function IR + caller hops + helper-exit leak. Oracle: `pause-tracking-helper`.
3. **`+=` / `++` writes.** Landed in v29. Unit + `no-side-effects-in-computed` / `prefer-computed` fixtures. Logical compounds stay quiet.
4. **`peel_parens` on watch sources.** Landed in v31. Same peel as `local_getter_parts`. Oracle: `watch-source-parens`. Nested arrays stay identifier-only.
5. **Render identifier getters.** Landed in v32. `function_like_body` + `local_getter_parts`. Units in `tests/render.rs`. Graph-vs-graph (no render `onTrack` in the oracle harness).

### Do next (measurement, no fact change)

6. Oracle case `computed(() => load())` next to `computed-fn-ref`, or document in `oracle/README.md` that graph-vs-graph is the gate for the call form.
7. ~~Put `watch-source-reactive-deep` in `oracle_cases_cover_known_hard_facts`.~~ Added with the v31 peel case.
8. ~~Fix `docs/quality-baselines.md` `reactivity-rules` to **4 / 5**.~~ Already matches.
9. Say "policy algebra" in the ExportState prose the next time that heading is edited.

### Do when touching locality (faster, already #108)

10. Make `DirtyPlan.export_closure` the linker dirty set, or stop claiming it is one. Session should not reconstruct every `ModuleSource` on an independent leaf edit.
11. Compute `local_zero_arg_callees_in_scope` once per `finish_scope` and pass the set into reads / uncertain / writes. No fact change if the set is identical. Bench `trace_1k_modules` before/after.
12. Keep `persist_linking_cache` off for one-shot benches. Add one CodSpeed name that *is* a warm `ModuleTraceState` leaf edit if locality work lands.

### Consumer polish (smarter without AST)

13. File-scoped explain (`file:@offset` should not walk every `module_reactivity` entry). Binding → inbound edges index for TUI/VS Code.
14. VS Code Explain Scope should hit the LSP session, not `--no-cache` CLI. MCP `vue_vet_scan` should keep the `reactivity` digest the CLI already ships, or document why agents do not get "tracer ran."

### Stop

- Pure extract of `summary/mod.rs` / `link.rs` / `reactivity_tui.rs`.
- IFDS/IDE rewrite, oxc_cfg A4, Willow typechecker, Vapor-shaped IR.
- Deeper A4 as a recall project.
- Allowlist growth without oracle or a named real-app FP.
- Treating the 280 corpus or the 99% gate as app-level science.

## Distillation for the returning human

Unstamped. Nothing here is vouched.

**Keep.** Under-approx, static-only, quiet failure, plugin-supplied bags, shared `follow_local_callees`, identifier getters, `ModuleTraceState` plan equality, oracle as the precision ruler.

**The interesting remaining bug class** after v32 is remaining dual-path writes (composable-instance / HOF writes), not another Factory seed. Helper context, `+=` / `++` writes, watch-source peel, and render identifier getters are closed.

**The interesting remaining speed class** is session input breadth and repeated callee discovery, not a new interprocedural framework.

**The interesting remaining science class** is honesty: oracle covers a slice; `ExportState` is policy; §K of the July harvest was a year of implementation behind.

**Conflict with vouched PCR.** None on direction. The July literature §K "current state" column contradicted the v27 PCR; this memo + the §K refresh resolve that by updating the unstamped research layer. A4 "no further CF depth" still stands for *recall*. Caller-guard-on-follow is dual-path hygiene, the same rule that produced v25–v27.
