# Vapor SFC compile contract

Default severity: **info**. Category: **migration** (excluded from score and default CI exit). Group: **vapor-migration**. Opt-in.

One finding per compile-contract reason, on the offending construct.

## Audited toolchain tuple

Same as [`vapor-assessment`](./vapor-assessment.md): Vue `3.6.0-rc.7` and `@vitejs/plugin-vue` `6.0.8`.

## Verdict table

| Reason | Verdict | Span |
| --- | --- | --- |
| Ordinary `<script>` only (no `<script setup>`, no `vapor` attr) | `blocked` | `<script>` open tag |
| `<script vapor>` containing `export` / `export default` | `blocked` | the export statement |
| Dual script (`<script>` + `<script setup>`) | `needs-verification` | ordinary `<script>` open tag |
| `<template vapor>` + Options script without setup | `needs-verification` | ordinary `<script>` open tag |
| Template-only or `<script setup>` ± `vapor` | `compiler-candidate` | (no extra finding) |

`<script vapor>` is treated as setup: Vue flips that block to setup.

## Why

plugin-vue force-conversion and Vapor setup assembly reject ordinary-script-only SFCs and runtime exports from a vapor/setup block.

## Safe patterns

Use `<script setup>` (optionally with `vapor`) or a template-only SFC. Keep Options API companions off the Vapor path until verified.
