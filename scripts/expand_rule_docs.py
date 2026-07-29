#!/usr/bin/env python3
"""Expand stub rule docs under docs/rules/ into readable Bad/Good pages."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

ID_RE = re.compile(r'id:\s*"(vue-vet/(?P<cat>[^/"]+)/(?P<name>[^"]+))"')
DOC_RE = re.compile(r'documentation:\s*"(?P<doc>rules/[^"]+)"')
PAIR_RE = re.compile(
  r'"(vue-vet/(?P<cat>[^/"]+)/(?P<name>[^"]+))"\s*,\s*"(?P<doc>rules/[^"]+)"'
)
AFTER_AWAIT_RE = re.compile(
  r'after_await_call_rule!\(\s*\w+\s*,\s*"(vue-vet/correctness/no-(?P<slug>[^"]+)-after-await)"\s*,\s*"[^"]+"\s*,\s*"(?P<callee>[^"]+)"'
)
# graph_extra / directives macros
MACRO_AFTER_RE = re.compile(
  r'macro_after_await!\(\s*\w+\s*,\s*\w+\s*,\s*"(vue-vet/correctness/[^"]+)"\s*,\s*"[^"]+"\s*,\s*"(?P<callee>[^"]+)"'
)
BOUNDARY_RE = re.compile(
  r'boundary_rule!\(\s*\w+\s*,\s*"(vue-vet/reactivity/(?P<name>[^"]+))"\s*,\s*"[^"]+"\s*,\s*(?P<scope>\w+)\s*,\s*(?P<read>\w+)\s*,\s*"(?P<label>[^"]+)"\s*,\s*"(?P<reason>[^"]+)"'
)
MISSING_EXPR_RE = re.compile(
  r'missing_expr!\(\s*\w+\s*,\s*"(vue-vet/correctness/(?P<name>[^"]+))"\s*,\s*"[^"]+"\s*,\s*"(?P<dir>[^"]+)"'
)
DESTRUCTURE_RE = re.compile(
  r'destructure_rule!\(\s*\w+\s*,\s*"(vue-vet/reactivity/(?P<name>[^"]+))"\s*,\s*"[^"]+"\s*,\s*"(?P<source>[^"]*)"\s*,\s*"(?P<message>[^"]+)"\s*,\s*"(?P<help>[^"]+)"'
)
REF_OPERAND_RE = re.compile(
  r'ref_operand_rule!\(\s*\w+\s*,\s*"(vue-vet/reactivity/(?P<name>[^"]+))"\s*,\s*"[^"]+"\s*,\s*"(?P<label>[^"]+)"'
)


def collect_rules() -> dict[str, str]:
  rules: dict[str, str] = {}
  for directory in (
    ROOT / "crates/vue_vet_rules/src",
    ROOT / "crates/vue_vet_practice/src",
    ROOT / "crates/vue_vet_project/src",
  ):
    for path in directory.rglob("*.rs"):
      text = path.read_text()
      for match in PAIR_RE.finditer(text):
        rules[f"vue-vet/{match['cat']}/{match['name']}"] = match["doc"]
      for id_match in ID_RE.finditer(text):
        window = text[id_match.start() : id_match.start() + 500]
        doc_match = DOC_RE.search(window)
        if doc_match:
          rules[id_match.group(0).split('"')[1]] = doc_match["doc"]
  return rules


def parse_extras() -> dict[str, dict[str, str]]:
  extras: dict[str, dict[str, str]] = {}
  matrix = (ROOT / "crates/vue_vet_rules/src/rules/matrix/mod.rs").read_text()
  for match in AFTER_AWAIT_RE.finditer(matrix):
    rid = f"vue-vet/correctness/no-{match['slug']}-after-await"
    extras[rid] = {"family": "after_await", "callee": match["callee"]}
  for match in BOUNDARY_RE.finditer(matrix):
    rid = f"vue-vet/reactivity/{match['name']}"
    extras[rid] = {
      "family": "boundary",
      "scope": match["scope"],
      "read": match["read"],
      "label": match["label"],
      "reason": match["reason"],
    }
  for match in DESTRUCTURE_RE.finditer(matrix):
    rid = f"vue-vet/reactivity/{match['name']}"
    extras[rid] = {
      "family": "destructure",
      "source": match["source"] or "reactive object",
      "message": match["message"],
      "help": match["help"],
    }
  for match in REF_OPERAND_RE.finditer(matrix):
    rid = f"vue-vet/reactivity/{match['name']}"
    extras[rid] = {"family": "ref_operand", "label": match["label"]}

  directives = (ROOT / "crates/vue_vet_rules/src/rules/directives/mod.rs").read_text()
  for match in MISSING_EXPR_RE.finditer(directives):
    rid = f"vue-vet/correctness/{match['name']}"
    extras[rid] = {"family": "missing_expr", "directive": match["dir"]}

  graph = (ROOT / "crates/vue_vet_rules/src/rules/graph_extra.rs").read_text()
  for match in MACRO_AFTER_RE.finditer(graph):
    # recover id from nearby - MACRO_AFTER has id as third arg
    pass
  for match in re.finditer(
    r'macro_after_await!\(\s*\w+\s*,\s*\w+\s*,\s*"(?P<id>vue-vet/correctness/[^"]+)"\s*,\s*"[^"]+"\s*,\s*"(?P<callee>[^"]+)"',
    graph,
  ):
    extras[match["id"]] = {"family": "after_await_macro", "callee": match["callee"]}

  return extras


PLACEHOLDER_SNIPPET = "const x = ref(0)"


def is_stub(text: str) -> bool:
  stripped = text.strip()
  if len(stripped) < 250 and "```vue" not in stripped:
    return True
  if "Vue Vet matrix rule" in stripped:
    return True
  if "Built-in rule `" in stripped and len(stripped) < 350:
    return True
  if "See `fixtures/rules/" in stripped and "```vue" not in stripped:
    return True
  # Prior expand pass embedded generic placeholder fixtures
  if PLACEHOLDER_SNIPPET in stripped and stripped.count("```vue") >= 1:
    # Allow real rules that legitimately use `x`; require both Bad+Good trivial
    if stripped.count(PLACEHOLDER_SNIPPET) >= 2:
      return True
  if "Moved\n\nThis rule is documented at" in stripped:
    return False
  return False


def fence(source: str) -> str:
  body = source.strip("\n")
  return f"```vue\n{body}\n```"


def load_fixture(name: str, kind: str, *, require: set[str] | None = None) -> str | None:
  directory = ROOT / "fixtures/rules" / name / kind
  if not directory.is_dir():
    return None
  files = sorted(directory.glob("*.vue"))
  if not files:
    return None
  preferred = [f for f in files if "placeholder" not in f.name]
  chosen = preferred[0] if preferred else files[0]
  text = chosen.read_text()
  if "placeholder" in chosen.name:
    return None
  stripped = text.strip()
  # Generic unused placeholder used across many new rules
  if stripped in {
    '<script setup lang="ts">\nimport { ref } from \'vue\'\nconst x = ref(0)\n</script>\n<template>{{ x }}</template>',
    "<script setup lang=\"ts\">\nimport { ref } from 'vue'\nconst x = ref(0)\n</script>\n<template>{{ x }}</template>",
  }:
    return None
  if require and not any(token in text for token in require):
    return None
  if len(stripped) < 40:
    return None
  return text


def title_case(name: str) -> str:
  return name.replace("-", " ").title()


def synthetic_after_await(callee: str) -> tuple[str, str]:
  bad = f"""<script setup lang="ts">
import {{ {callee} }} from 'vue'
const data = await fetch('/api').then((response) => response.json())
{callee}(() => {{
  console.log(data)
}})
</script>

<template>
  <div />
</template>"""
  if callee.startswith("define") or callee in {
    "provide",
    "inject",
    "useAttrs",
    "useSlots",
    "useCssModule",
    "useCssVars",
    "withDefaults",
    "getCurrentInstance",
    "nextTick",
    "effectScope",
  }:
    if callee == "defineProps":
      bad = """<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
const props = defineProps<{ title: string }>()
</script>

<template>
  <p>{{ props.title }} {{ data }}</p>
</template>"""
    elif callee == "withDefaults":
      bad = """<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
const props = withDefaults(defineProps<{ title?: string }>(), { title: 'hi' })
</script>

<template>
  <p>{{ props.title }} {{ data }}</p>
</template>"""
    elif callee.startswith("define"):
      bad = f"""<script setup lang="ts">
const data = await fetch('/api').then((response) => response.json())
{callee}()
</script>

<template>
  <div>{{{{ data }}}}</div>
</template>"""
    elif callee in {"provide", "inject"}:
      bad = f"""<script setup lang="ts">
import {{ {callee} }} from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = {callee}('key'{', 1' if callee == 'provide' else ''})
</script>

<template>
  <div>{{{{ data }}}} {{{{ value }}}}</div>
</template>"""
    else:
      bad = f"""<script setup lang="ts">
import {{ {callee} }} from 'vue'
const data = await fetch('/api').then((response) => response.json())
const value = {callee}()
</script>

<template>
  <div>{{{{ data }}}} {{{{ value }}}}</div>
</template>"""

  good = f"""<script setup lang="ts">
import {{ {callee} }} from 'vue'
{callee}(() => {{
  console.log('ready')
}})
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{{{ data }}}}</div>
</template>"""
  if callee.startswith("define") or callee in {
    "provide",
    "inject",
    "useAttrs",
    "useSlots",
    "useCssModule",
    "useCssVars",
    "withDefaults",
    "getCurrentInstance",
  }:
    if callee == "defineProps":
      good = """<script setup lang="ts">
const props = defineProps<{ title: string }>()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <p>{{ props.title }} {{ data }}</p>
</template>"""
    elif callee == "withDefaults":
      good = """<script setup lang="ts">
const props = withDefaults(defineProps<{ title?: string }>(), { title: 'hi' })
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <p>{{ props.title }} {{ data }}</p>
</template>"""
    elif callee.startswith("define"):
      good = f"""<script setup lang="ts">
{callee}()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{{{ data }}}}</div>
</template>"""
    elif callee == "provide":
      good = """<script setup lang="ts">
import { provide } from 'vue'
provide('key', 1)
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }}</div>
</template>"""
    elif callee == "inject":
      good = """<script setup lang="ts">
import { inject } from 'vue'
const value = inject('key')
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{ data }} {{ value }}</div>
</template>"""
    else:
      good = f"""<script setup lang="ts">
import {{ {callee} }} from 'vue'
const value = {callee}()
const data = await fetch('/api').then((response) => response.json())
</script>

<template>
  <div>{{{{ data }}}}</div>
</template>"""
  return bad, good


def synthetic_boundary(label: str, reason: str, scope: str) -> tuple[str, str, str]:
  summary = (
    f"Reports reactive reads inside `{label}` that happen {reason}. "
    "Those reads are not stable dependencies for the tracking scope."
  )
  if "Conditional" in scope or "conditional" in reason or "guard" in reason:
    bad = """<script setup lang="ts">
import { computed, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
const label = computed(() => {
  if (!enabled.value) return 'off'
  return String(count.value)
})
</script>

<template>
  <p>{{ label }}</p>
</template>"""
    good = """<script setup lang="ts">
import { computed, ref } from 'vue'
const enabled = ref(false)
const count = ref(0)
const label = computed(() => (enabled.value ? String(count.value) : 'off'))
</script>

<template>
  <p>{{ label }}</p>
</template>"""
  elif "AfterAwait" in scope or "await" in reason:
    bad = """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(async () => {
  await Promise.resolve()
  return String(count.value)
})
</script>

<template>
  <p>{{ label }}</p>
</template>"""
    good = """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => String(count.value))
</script>

<template>
  <p>{{ label }}</p>
</template>"""
  else:
    bad = """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => {
  const later = () => count.value
  return later()
})
</script>

<template>
  <p>{{ label }}</p>
</template>"""
    good = """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => count.value)
</script>

<template>
  <p>{{ label }}</p>
</template>"""
  return summary, bad, good


def synthetic_missing_expr(directive: str) -> tuple[str, str, str]:
  summary = f"`v-{directive}` must include a non-empty expression."
  bad = f"""<template>
  <div v-{directive}="">content</div>
</template>"""
  good = f"""<script setup lang="ts">
import {{ ref }} from 'vue'
const value = ref(true)
</script>

<template>
  <div v-{directive}="value">content</div>
</template>"""
  return summary, bad, good


def family_content(rid: str, name: str, extras: dict[str, str]) -> tuple[str, str, str, str]:
  """Returns summary, bad, good, remediation."""
  family = extras.get("family", "")

  if family in {"after_await", "after_await_macro"}:
    callee = extras["callee"]
    summary = (
      f"In `<script setup>`, calling `{callee}` after a top-level `await` runs outside "
      "the synchronous setup instance context, so the API will not bind correctly."
    )
    bad, good = synthetic_after_await(callee)
    remediation = f"Move `{callee}` before the first top-level `await`."
    return summary, bad, good, remediation

  if family == "boundary":
    summary, bad, good = synthetic_boundary(extras["label"], extras["reason"], extras["scope"])
    remediation = (
      f"Keep reactive reads synchronous and unconditional inside `{extras['label']}`, "
      "or switch to an API with explicit sources (`watch([...])`)."
    )
    return summary, bad, good, remediation

  if family == "missing_expr":
    summary, bad, good = synthetic_missing_expr(extras["directive"])
    return summary, bad, good, f"Provide an expression for `v-{extras['directive']}`."

  if family == "destructure":
    source = extras["source"] or "reactive(...)"
    summary = extras["message"][0].upper() + extras["message"][1:]
    bad = f"""<script setup lang="ts">
import {{ reactive }} from 'vue'
const {{ count }} = {source + '()' if source.isidentifier() else 'reactive({ count: 0 })'}
</script>

<template>
  <p>{{{{ count }}}}</p>
</template>"""
    if source == "reactive":
      bad = """<script setup lang="ts">
import { reactive } from 'vue'
const { count } = reactive({ count: 0 })
</script>

<template>
  <p>{{ count }}</p>
</template>"""
      good = """<script setup lang="ts">
import { reactive, toRefs } from 'vue'
const state = reactive({ count: 0 })
const { count } = toRefs(state)
</script>

<template>
  <p>{{ count }}</p>
</template>"""
    else:
      good = f"""<script setup lang="ts">
import {{ toRefs }} from 'vue'
// Keep the reactive object and read through it, or use toRefs / storeToRefs.
const state = /* {source} */ ({{ count: 0 }} as any)
const {{ count }} = toRefs(state)
</script>

<template>
  <p>{{{{ count }}}}</p>
</template>"""
    return summary, bad, good, extras["help"]

  if family == "ref_operand":
    label = extras["label"]
    summary = (
      f"Using a {label} object directly as an operand reads the object wrapper, "
      "not the inner value. Unwrap with `.value` (or `toValue`)."
    )
    bad = """<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>"""
    good = """<script setup lang="ts">
import { ref } from 'vue'
const count = ref(0)
const ok = count.value > 0
</script>

<template>
  <p>{{ ok }}</p>
</template>"""
    return summary, bad, good, f"Read `{label}.value` (or `toValue(...)`) at the use site."

  # Named one-offs
  one_offs: dict[str, tuple[str, str, str, str]] = {
    "no-child-content": (
      "Elements that take text via an attribute (for example `v-text` / `v-html` / `textarea` value patterns) "
      "should not also carry child content that the attribute replaces.",
      """<template>
  <div v-text="msg">also children</div>
</template>""",
      """<template>
  <div v-text="msg" />
</template>""",
      "Remove the child nodes, or drop the attribute and keep children.",
    ),
    "no-deprecated-filter": (
      "Vue 3 removed filters. The `|` filter syntax in templates is invalid.",
      """<template>
  <p>{{ msg | capitalize }}</p>
</template>""",
      """<template>
  <p>{{ capitalize(msg) }}</p>
</template>""",
      "Replace filters with methods, computed properties, or plain functions.",
    ),
    "no-deprecated-slot-attribute": (
      "The `slot` attribute is Vue 2 syntax. Prefer `v-slot` / `#`.",
      """<template>
  <Comp><div slot="header">Title</div></Comp>
</template>""",
      """<template>
  <Comp><template #header>Title</template></Comp>
</template>""",
      "Migrate to `v-slot` / named slot shorthand.",
    ),
    "no-deprecated-v-bind-sync": (
      "`.sync` is Vue 2 sugar. Prefer `v-model:prop` in Vue 3.",
      """<template>
  <Comp :title.sync="title" />
</template>""",
      """<template>
  <Comp v-model:title="title" />
</template>""",
      "Replace `.sync` with `v-model` arguments.",
    ),
    "no-duplicate-attributes": (
      "Duplicate attributes on the same element are ambiguous.",
      """<template>
  <div id="a" id="b" />
</template>""",
      """<template>
  <div id="a" />
</template>""",
      "Keep a single attribute of each name (or merge bindings intentionally).",
    ),
    "no-duplicate-define-model": (
      "Calling `defineModel` twice for the same model name is invalid.",
      """<script setup lang="ts">
const model = defineModel<string>()
const again = defineModel<string>()
</script>""",
      """<script setup lang="ts">
const model = defineModel<string>()
</script>""",
      "Keep one `defineModel` per model name.",
    ),
    "no-import-compiler-macros": (
      "Compiler macros (`defineProps`, `defineEmits`, …) are compiler-injected and must not be imported.",
      """<script setup lang="ts">
import { defineProps } from 'vue'
const props = defineProps<{ title: string }>()
</script>""",
      """<script setup lang="ts">
const props = defineProps<{ title: string }>()
</script>""",
      "Delete the import; call the macro directly in `<script setup>`.",
    ),
    "no-dupe-v-else-if": (
      "Duplicate `v-else-if` conditions in the same chain are dead / confusing.",
      """<template>
  <div v-if="a" />
  <div v-else-if="a" />
</template>""",
      """<template>
  <div v-if="a" />
  <div v-else-if="b" />
</template>""",
      "Use distinct conditions or collapse branches.",
    ),
    "no-template-key": (
      "`<template>` special elements should not carry `key` the way elements do; put `key` on real elements / `v-for` sources as Vue expects.",
      """<template>
  <template key="x"><div /></template>
</template>""",
      """<template>
  <div key="x" />
</template>""",
      "Move `key` onto the keyed element or the `v-for` node Vue documents.",
    ),
    "no-textarea-mustache": (
      "Interpolation inside `<textarea>` is not the Vue 3 control surface; bind with `v-model`.",
      """<template>
  <textarea>{{ text }}</textarea>
</template>""",
      """<template>
  <textarea v-model="text" />
</template>""",
      "Use `v-model` (or `:value` + listeners) instead of mustache children.",
    ),
    "no-v-text-v-html-on-component": (
      "`v-text` / `v-html` on components do not reliably set component content.",
      """<template>
  <Comp v-html="html" />
</template>""",
      """<template>
  <div v-html="html" />
</template>""",
      "Apply `v-html` / `v-text` to native elements, or pass content through slots/props.",
    ),
    "require-toggle-inside-transition": (
      "`<Transition>` children need a toggle surface (`v-if` / `v-show`) to animate enter/leave.",
      """<template>
  <Transition><div>always</div></Transition>
</template>""",
      """<template>
  <Transition><div v-if="open">shown</div></Transition>
</template>""",
      "Wrap a conditionally rendered / shown element inside the transition.",
    ),
    "valid-v-else": (
      "`v-else` must immediately follow a `v-if` / `v-else-if` chain.",
      """<template>
  <div v-else />
</template>""",
      """<template>
  <div v-if="a" />
  <div v-else />
</template>""",
      "Attach `v-else` to a valid chain.",
    ),
    "valid-v-bind": (
      "`v-bind` / `:` requires an expression (unless using the object form correctly).",
      """<template>
  <div :id="" />
</template>""",
      """<template>
  <div :id="id" />
</template>""",
      "Provide a binding expression.",
    ),
    "valid-v-slot": (
      "`v-slot` / `#` must be used on components or `<template>` slot outlets with a valid target.",
      """<template>
  <div v-slot:header>x</div>
</template>""",
      """<template>
  <Comp><template #header>x</template></Comp>
</template>""",
      "Put slot syntax on a component / `<template>` slot outlet.",
    ),
    "valid-v-on": (
      "`v-on` / `@` needs an event name and a handler expression.",
      """<template>
  <button v-on:="" />
</template>""",
      """<template>
  <button @click="onClick" />
</template>""",
      "Provide an event name and handler.",
    ),
    "no-self-trigger-in-watch-effect": (
      "`watchEffect` that writes a dependency it also reads can loop.",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
watchEffect(() => {
  count.value = count.value + 1
})
</script>""",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const count = ref(0)
const doubled = ref(0)
watchEffect(() => {
  doubled.value = count.value * 2
})
</script>""",
      "Write a different binding, or use `watch` with explicit sources.",
    ),
    "no-side-effects-in-computed": (
      "`computed` getters should be pure. Side effects belong in `watch` / lifecycle hooks.",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => {
  console.log('tick')
  return count.value
})
</script>""",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => count.value)
</script>""",
      "Move side effects out of the computed getter.",
    ),
    "no-computed-without-dependency": (
      "A `computed` that never reads reactive state is just a static value.",
      """<script setup lang="ts">
import { computed } from 'vue'
const label = computed(() => 'static')
</script>""",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => String(count.value))
</script>""",
      "Return a plain value, or read reactive state inside the getter.",
    ),
    "no-effect-write-without-read": (
      "`watchEffect` that only writes and never reads reactive state will not re-run usefully.",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const out = ref(0)
watchEffect(() => {
  out.value = 1
})
</script>""",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
const out = ref(0)
watchEffect(() => {
  out.value = source.value
})
</script>""",
      "Read the inputs you depend on, or use a one-shot assignment outside an effect.",
    ),
    "prefer-watch-over-effect-for-single-source": (
      "An assignment-only `watchEffect` that tracks a single unconditional source is clearer as `watch`.",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const source = ref(0)
const out = ref(0)
watchEffect(() => {
  out.value = source.value
})
</script>""",
      """<script setup lang="ts">
import { ref, watch } from 'vue'
const source = ref(0)
const out = ref(0)
watch(source, (value) => {
  out.value = value
})
</script>""",
      "Prefer `watch(source, ...)` for single-source sync.",
    ),
    "prefer-explicit-sources-for-conditional-deps": (
      "Conditional reactive reads inside effects are clearer with explicit `watch` sources.",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const enabled = ref(false)
const result = ref(0)
watchEffect(() => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>""",
      """<script setup lang="ts">
import { ref, watch } from 'vue'
const enabled = ref(false)
const result = ref(0)
watch([enabled, result], () => {
  if (!enabled.value) return
  console.log(result.value)
})
</script>""",
      "List every dependency in `watch([...])`.",
    ),
    "no-multiple-effects-same-target": (
      "Multiple effects writing the same target race updates.",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const a = ref(1)
const b = ref(2)
const out = ref(0)
watchEffect(() => { out.value = a.value })
watchEffect(() => { out.value = b.value })
</script>""",
      """<script setup lang="ts">
import { ref, watchEffect } from 'vue'
const a = ref(1)
const out = ref(0)
watchEffect(() => { out.value = a.value })
</script>""",
      "Keep a single writer, or write distinct targets.",
    ),
    "no-props-snapshot-in-ref": (
      "Wrapping `props` fields in `ref(props.x)` snapshots the current value and drops prop reactivity.",
      """<script setup lang="ts">
import { ref } from 'vue'
const props = defineProps<{ title: string }>()
const title = ref(props.title)
</script>""",
      """<script setup lang="ts">
import { toRef } from 'vue'
const props = defineProps<{ title: string }>()
const title = toRef(props, 'title')
</script>""",
      "Use `toRef` / `toRefs`, or read `props.title` directly.",
    ),
    "no-v-model-nonreactive-source": (
      "`v-model` should bind a reactive script value.",
      """<script setup lang="ts">
let text = ''
</script>
<template>
  <input v-model="text" />
</template>""",
      """<script setup lang="ts">
import { ref } from 'vue'
const text = ref('')
</script>
<template>
  <input v-model="text" />
</template>""",
      "Bind a `ref` / `computed` / reactive property.",
    ),
    "no-unused-computed-binding": (
      "A `computed` binding that is never read is dead work.",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>
<template>
  <p>{{ count }}</p>
</template>""",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const doubled = computed(() => count.value * 2)
</script>
<template>
  <p>{{ doubled }}</p>
</template>""",
      "Read the computed in script/template, or delete it.",
    ),
    "no-watch-callback-as-tracking-scope": (
      "`watch` callbacks are not tracking scopes. Reactive reads there do not subscribe like `watchEffect`.",
      """<script setup lang="ts">
import { ref, watch } from 'vue'
const a = ref(0)
const b = ref(0)
watch(a, () => {
  console.log(b.value)
})
</script>""",
      """<script setup lang="ts">
import { ref, watch } from 'vue'
const a = ref(0)
const b = ref(0)
watch([a, b], () => {
  console.log(b.value)
})
</script>""",
      "List every value you need in the watch source list.",
    ),
    "no-empty-watch-sources": (
      "`watch` with an empty source list never runs usefully.",
      """<script setup lang="ts">
import { watch } from 'vue'
watch([], () => {})
</script>""",
      """<script setup lang="ts">
import { ref, watch } from 'vue'
const count = ref(0)
watch(count, () => {})
</script>""",
      "Pass at least one source.",
    ),
    "no-readonly-mutation": (
      "Readonly projections must not be mutated.",
      """<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 0 })
const view = readonly(state)
view.count++
</script>""",
      """<script setup lang="ts">
import { reactive, readonly } from 'vue'
const state = reactive({ count: 0 })
const view = readonly(state)
state.count++
void view
</script>""",
      "Mutate the source reactive state instead.",
    ),
    "no-stale-prop-flow": (
      "Cross-file prop edges should start from reactive parent state; plain values go stale.",
      """<!-- parent -->
<script setup lang="ts">
let title = 'hi'
</script>
<template>
  <Child :title="title" />
</template>""",
      """<!-- parent -->
<script setup lang="ts">
import { ref } from 'vue'
const title = ref('hi')
</script>
<template>
  <Child :title="title" />
</template>""",
      "Pass a reactive binding (ref/computed/reactive field).",
    ),
  }

  if name in one_offs:
    return one_offs[name]

  # Pathology / leftover matrix names from slug heuristics
  if name.startswith("no-self-trigger"):
    return one_offs["no-self-trigger-in-watch-effect"]
  if name.startswith("no-computed-self-trigger"):
    return (
      "`computed` that writes a dependency it reads can self-trigger.",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => {
  count.value++
  return count.value
})
</script>""",
      """<script setup lang="ts">
import { computed, ref } from 'vue'
const count = ref(0)
const label = computed(() => count.value)
</script>""",
      "Keep computed getters pure.",
    )

  summary = (
    f"Vue Vet rule `{rid}` reports a fact-driven correctness or reactivity issue. "
    "Prefer the Bad/Good examples below; fixtures under "
    f"`fixtures/rules/{name}/` are the executable corpus."
  )
  bad = load_fixture(name, "invalid") or """<script setup lang="ts">
// See fixtures/rules for the executable invalid corpus.
</script>"""
  good = load_fixture(name, "valid") or """<script setup lang="ts">
// See fixtures/rules for the executable valid corpus.
</script>"""
  return summary, bad, good, "Follow the Good pattern, or suppress with a narrow inline disable when reviewed."


def render(rid: str, doc: str, extras: dict[str, str]) -> str:
  cat = rid.split("/")[1]
  name = rid.rsplit("/", 1)[-1]
  severity = "warning"
  if extras.get("family") == "missing_expr" or name.startswith("valid-"):
    severity = "error"
  if cat == "practice":
    severity = "info"

  summary, bad, good, remediation = family_content(rid, name, extras)
  family = extras.get("family", "")
  require_bad: set[str] | None = None
  if family in {"after_await", "after_await_macro"}:
    require_bad = {"await", extras.get("callee", "")}
  elif family == "missing_expr":
    require_bad = {f"v-{extras.get('directive', '')}", f":{extras.get('directive', '')}"}
  elif family == "ref_operand":
    require_bad = {">", "<", "+", "-", "*", "/", "!", "==", "!=", "&&", "||"}

  fixture_bad = load_fixture(name, "invalid", require=require_bad)
  fixture_good = load_fixture(name, "valid")
  # Prefer executable fixtures only when they actually demonstrate the rule.
  if fixture_bad and family in {"missing_expr", "ref_operand", "destructure", ""}:
    bad = fixture_bad
  if fixture_good and family in {"missing_expr", "ref_operand", "destructure", ""}:
    good = fixture_good

  return "\n".join(
    [
      f"# `{rid}`",
      "",
      f"Category: {cat}  ",
      f"Default severity: {severity}  ",
      "Confidence: high",
      "",
      summary,
      "",
      "## Bad",
      "",
      fence(bad),
      "",
      "## Good",
      "",
      fence(good),
      "",
      "## Detection",
      "",
      "Fact-driven via Vue Vet's Vize / Oxc / reactivity-graph facts "
      "(not a parallel regex pattern engine).",
      "",
      "## Remediation",
      "",
      remediation,
      "",
      "## Fixtures",
      "",
      f"- Invalid: `fixtures/rules/{name}/invalid/`",
      f"- Valid: `fixtures/rules/{name}/valid/`",
      "",
    ]
  )


def main() -> None:
  rules = collect_rules()
  extras_map = parse_extras()
  rewritten = 0
  skipped = 0
  for rid, doc in sorted(rules.items()):
    if rid.endswith("/no-mutating-props") and doc.endswith("reactivity/no-mutating-props"):
      # keep redirect stub
      skipped += 1
      continue
    path = ROOT / "docs" / f"{doc}.md"
    existing = path.read_text() if path.exists() else ""
    # Always rewrite known stubs; leave polished essays alone
    if path.exists() and not is_stub(existing):
      # still rewrite redirect-only or autofocus-style generic boilerplate?
      if "This high-confidence recommended rule reports a concrete Vue" in existing:
        pass  # leave older generic essays for now unless short
      else:
        skipped += 1
        continue
    extras = extras_map.get(rid, {})
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render(rid, doc, extras))
    rewritten += 1
  print(f"rewrote {rewritten} docs; skipped {skipped}")


if __name__ == "__main__":
  main()
