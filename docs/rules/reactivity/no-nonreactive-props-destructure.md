# Preserve destructured props reactivity before Vue 3.5

Vue 3.4 and older treat a binding destructured directly from `defineProps()` as a constant. Vue
3.5 added a compiler transform that keeps the same syntax reactive inside the same
`<script setup>` block.

## Bad on Vue 3.4 and older

```vue
const { title } = defineProps<{ title: string }>()
```

## Good on Vue 3.4 and older

```vue
const props = defineProps<{ title: string }>()
const { title } = toRefs(props)
```

Direct destructuring is also accepted by this rule on Vue 3.5 and newer.

## Limitations

The rule runs only when Vue Vet can parse a numeric `vue` requirement from the nearest
`package.json`. Unknown versions stay quiet. Passing a destructured prop as a reactive source is
a separate concern: use a getter such as `() => title`.

## Remediation

Use `toRefs(props)`, keep access through the props object, or upgrade to Vue 3.5+ when the compiler
transform fits the project.
