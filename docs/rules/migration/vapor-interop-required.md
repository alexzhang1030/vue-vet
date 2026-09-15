# Vapor interop required

Default severity: **info**. Category: **migration** (excluded from score and default CI exit). Group: **vapor-migration**. Opt-in.

One finding per built-in interop tag (`Suspense`, `Teleport`, `KeepAlive`, `Transition`, `TransitionGroup`) and per resolved child component whose SFC is not vapor-opted-in. Unresolved child tags add an unknown reason and no finding.

## Audited toolchain tuple

Same as [`vapor-assessment`](./vapor-assessment.md): Vue `3.6.0-rc.7` and `@vitejs/plugin-vue` `6.0.8`.

## Verdict table

| Surface | Verdict |
| --- | --- |
| Built-in interop tag or resolved non-vapor child | `needs-verification` (`requires vaporInteropPlugin; not executed in the audited envelope`) |
| Unresolved child component | unknown `child component <Tag> unresolved` (no diagnostic) |
| No children or built-ins | `not-applicable` |

`MyTeleport` is not `Teleport`. A `Transition` token inside a string or attribute value is not a tag.

## Why

Vapor interop with VDOM built-ins and non-vapor children requires `vaporInteropPlugin`, which this assessment does not execute.

## Safe patterns

Keep built-ins and non-vapor children off the assessed SFC, or mark child SFCs with `vapor` when they are opted in.
