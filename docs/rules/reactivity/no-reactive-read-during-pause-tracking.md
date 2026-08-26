# `vue-vet/reactivity/no-reactive-read-during-pause-tracking`

Category: reactivity  
Default severity: warning  
Confidence: high  
Tier: tracer

Reads between `pauseTracking()` and `enableTracking()` / `resetTracking()` are
outside synchronous dependency collection. The effect will not re-run when those
values change.

## Bad

```vue
<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  pauseTracking()
  console.log(value.value)
  enableTracking()
})
</script>
```

## Good

```vue
<script setup lang="ts">
import { enableTracking, pauseTracking, ref, watchEffect } from 'vue'

const value = ref(0)
watchEffect(() => {
  const current = value.value
  pauseTracking()
  enableTracking()
  console.log(current)
})
</script>
```

Same dual-path applies when the pause lives in a same-file zero-arg helper:
`watchEffect(() => { void load() })` with
`function load() { pauseTracking(); return value.value }` is OutsideTracking on
`value`, matching the inlined window. A later `enableTracking()` after `load()`
returns still tracks sibling reads. `load(); pauseTracking(); load()` stays
Unconditional — Vue tracks the first call.

## Detection

Fact-driven: tracking-scope reads classified `OutsideTracking` when the script
calls `pauseTracking`.

## Remediation

Read dependencies before pausing tracking, or list them in explicit `watch` sources.

## Fixtures

- Invalid: `fixtures/rules/no-reactive-read-during-pause-tracking/invalid/`
  (`paused.vue`, `helper-pause.vue`, `caller-pause-helper.vue`)
- Valid: `fixtures/rules/no-reactive-read-during-pause-tracking/valid/`
  (`outside-pause.vue`, `helper-resume.vue`, `unpaused-then-paused-call.vue`)
