# Require accessible heading content

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<h1 />
<h2><div class="i-carbon-warning" /></h2>
```

## Good

```vue
<h1>Settings</h1>
<h2 aria-label="Warning"><div class="i-carbon-warning" /></h2>
<h2 title="Warning"><div class="i-carbon-warning" /></h2>
```

## Limitations

Accessible content means non-whitespace text, interpolation, `v-text`/`v-html`, or a descendant `img`/`area` with a non-empty `alt`. Element-only children and `aria-hidden` subtrees do not count. `aria-label`, `aria-labelledby`, and a nonempty HTML `title` / `:title` (HTML-AAM fallback) name the heading. An empty `title=""` does not. Named headings are not reported; there is no title-to-`aria-label` safe edit.

## Remediation

Add text content, an image with `alt`, or an `aria-label` / `aria-labelledby` binding.
