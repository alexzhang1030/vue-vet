# Keep focusable elements in the accessibility tree

A keyboard-focusable element with `aria-hidden="true"` can receive focus while remaining invisible to assistive technology.

## Bad

```vue
<button aria-hidden="true">Save</button>
<button :aria-hidden="true">Save</button>
```

## Good

```vue
<button>Save</button>
<div aria-hidden="true">Decorative duplicate</div>
<button disabled aria-hidden="true">Save</button>
<input type="hidden" aria-hidden="true">
```

## Limitations

Bound expressions other than the literal `true` stay quiet. Unquoted
`aria-hidden=true` is still reported, but the name-only fact span does not
cover a complete replacement, so no safe edit is offered.

## Remediation

Remove `aria-hidden` or remove the element from interaction.

Quoted `aria-hidden="true"`, `:aria-hidden="true"`, and
`v-bind:aria-hidden="true"` findings carry a safe edit that deletes the full
attribute (including the quoted value) when source reconstruction succeeds. Use
`--fix-dry-run` to inspect the range or `--fix-safe` to apply it and rescan.
