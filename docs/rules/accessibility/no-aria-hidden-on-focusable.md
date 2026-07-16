# Keep focusable elements in the accessibility tree

A keyboard-focusable element with `aria-hidden="true"` can receive focus while remaining invisible to assistive technology.

## Bad

```vue
<button aria-hidden="true">Save</button>
```

## Good

```vue
<button>Save</button>
<div aria-hidden="true">Decorative duplicate</div>
```

## Remediation

Remove `aria-hidden` or remove the element from interaction.
