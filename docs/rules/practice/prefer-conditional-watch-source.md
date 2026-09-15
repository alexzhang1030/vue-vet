# Prefer a guarded watch source for an idle computed producer

Default severity: **info**. Category: **practice** (excluded from score and default CI exit). Group: **derivation**.

`watch([flag, heavy], ...)` always tracks `heavy`. While `flag` is inactive,
writes to `heavy`'s dependency still evaluate that computed. When the callback
only assigns a private primitive sink under the guard, a getter tuple that
keeps the discriminator skips those idle evaluations.

A scalar fallback such as `flag.value ? heavy.value : 0` drops activation when
the selected value equals the sentinel. Keep `[flag.value, flag.value ? heavy.value : undefined]`.

## Bad

```vue
<script setup>
import { computed, ref, watch } from 'vue'

const flag = ref(false)
const source = ref(2)
const sink = ref(5)
const heavy = computed(() => source.value)
watch([flag, heavy], ([active, value]) => {
  if (active) sink.value = value
})
source.value = 3
</script>
```

## Good

```vue
<script setup>
import { computed, ref, watch } from 'vue'

const flag = ref(false)
const source = ref(2)
const sink = ref(5)
const heavy = computed(() => source.value)
watch(
  () => [flag.value, flag.value ? heavy.value : undefined],
  ([active, value]) => {
    if (active) sink.value = value
  },
)
source.value = 3
</script>
```

## Limitations

Requires a resolved Vue `watch` array, a tracked guard, a pure primitive local
computed, an actual dependency change while the guard is inactive, sole local
demand for that producer, and a callback whose full effects are a guarded
idempotent primitive sink assignment. Old-value logic, cleanup, `once` (including a bound `once` expression), other
producer subscribers (including template aliases), callback or helper escape,
dynamic branching, mutable prototypes or coercion, custom sink setters,
producer effects, debugger hooks (`onTrack` / `onTrigger`), unknown option
expressions, unknown helper effects on the guard, and uninvoked, short-circuited,
or conditional idle writes stay quiet. Preserve tuple slots, the guard, options,
and identity. No automatic rewrite.

## Remediation

Replace the array source with a getter tuple that includes the guard
discriminator. Keep the guarded assignment and existing flush / immediate
options.

## Fixtures

- Invalid: `fixtures/rules/prefer-conditional-watch-source/invalid/`
- Valid: `fixtures/rules/prefer-conditional-watch-source/valid/`
