# ADR 0002: Stable graph and evidence contracts

Status: accepted

## Decision

Vue Vet exposes two project-wide contracts owned by the workspace:

- `ProjectGraph` is the deterministic relation layer for nodes, edges, source
  provenance, invalidation inputs, and module reactivity.
- `EvidenceSummary` is the coverage layer for analysis results. It reports
  `complete`, `partial`, or `unavailable` and carries counted, stable gap codes.

The graph DTO has an explicit `PROJECT_GRAPH_SCHEMA_VERSION`. The evidence
model is shared by the session snapshot and all machine-facing reporters. Vize
and Oxc facts enter these contracts through adapters and remain outside the
public graph and evidence types.

## Rationale

Project rules, reactivity linking, session invalidation, and editor consumers
need the same relation identity and ordering. A graph version makes changes to
node, edge, or provenance semantics reviewable. A separate evidence status lets
consumers distinguish a clean complete scan from a clean result with bounded
analysis gaps.

## Consequences

- Graph nodes and edges remain sorted by their stable DTO ordering before they
  reach cache, reporters, or editor consumers.
- Recoverable parse and module-tracing issues become partial evidence with a
  counted gap. Fatal pipeline issues become unavailable evidence.
- JSON, MCP, and future LSP metadata can publish the same evidence contract.
- Graph schema changes require compatibility tests and a cache version review.
- Existing `project.complete` and `skipped_check_reasons` fields remain in the
  JSON v1 envelope while `evidence` becomes the typed coverage surface.
- Cache status remains host telemetry; a lookup miss also carries a stable
  `CacheRejection` when the reason is known. The deterministic JSON result keeps
  cache state outside its byte-comparison surface.

## Verification

- `vue_vet_core` tests cover gap ordering and merging.
- Reporter fixtures cover the additive JSON `evidence` field.
- Session tests cover evidence status derived from recoverable and fatal issues.
