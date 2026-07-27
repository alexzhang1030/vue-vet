# Require accessible button content

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<button type="button" />
<button type="button">
  <div class="i-carbon-close" />
</button>
```

## Good

```vue
<button type="button">Save</button>
<button type="button" aria-label="Close">
  <div class="i-carbon-close" />
</button>
```

## Limitations

Accessible content means non-whitespace text, interpolation, `v-text`/`v-html`, or a descendant `img`/`area` with a non-empty `alt`. Element-only children (icon wrappers) and `aria-hidden` subtrees do not count. `aria-label` and `aria-labelledby` on the button are accepted.

## Remediation

Add text content, an image with `alt`, or an `aria-label` / `aria-labelledby` binding.

