# Require accessible link content

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<a href="/settings" />
<a href="https://github.com" title="GitHub">
  <div class="i-carbon-logo-github" />
</a>
```

## Good

```vue
<a href="/settings">Settings</a>
<a href="https://github.com" aria-label="GitHub">
  <div class="i-carbon-logo-github" />
</a>
<a href="/docs"><img alt="Documentation" src="docs.png"></a>
```

## Limitations

Accessible content means non-whitespace text, interpolation, `v-text`/`v-html`, or a descendant `img`/`area` with a non-empty `alt`. Element-only children (icon wrappers) and `aria-hidden` subtrees do not count. `title` alone is not an accessible name. `aria-label` and `aria-labelledby` on the anchor are accepted. Runtime-rendered emptiness is not evaluated.

## Remediation

Add text content, an image with `alt`, or an `aria-label` / `aria-labelledby` binding.

