# Require a toggle directive inside `<transition>`

This high-confidence rule reports a `<transition>` wrapper whose content has no `v-if`, `v-show`, or dynamic `:is` toggle. `<transition>` only animates a state change; wrapping static content produces no transition at all and usually signals a missing toggle.

## Bad

```vue
<transition name="fade">
  <div>Always visible</div>
</transition>
```

## Good

```vue
<transition name="fade">
  <div v-if="visible">Sometimes visible</div>
</transition>
```

## Limitations

Facts expose no parent/child links, so this rule uses the next flat element entry as a best-effort proxy for the wrapped child (`collect_element` always pushes a parent before recursing into its own children, so the following entry is the first nested element in the common single-child case). `<transition>` wrappers whose only children are text/comments, or whose real child is not the very next element in document order, are not checked.

## Remediation

Add `v-if`, `v-show`, or a dynamic `:is` toggle on the transitioned child, or remove the `<transition>` wrapper if nothing ever toggles.
