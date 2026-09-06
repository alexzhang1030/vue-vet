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
<button type="button" title="Close">
  <div class="i-carbon-close" />
</button>
```

## Limitations

Accessible content means non-whitespace text, interpolation, `v-text`/`v-html`, or a descendant `img`/`area` with a non-empty `alt`. Element-only children (icon wrappers) and `aria-hidden` subtrees do not count. `aria-label`, `aria-labelledby`, and a nonempty HTML `title` / `:title` (HTML-AAM fallback) name the control. An empty `title=""` does not. Named controls are not reported; there is no title-to-`aria-label` safe edit.

## Remediation

Add text content, an image with `alt`, or an `aria-label` / `aria-labelledby` binding.
