# Require accessible link content

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<a href="/settings" />
<a href="https://github.com">
  <div class="i-carbon-logo-github" />
</a>
<RouterLink to="/">
  <div class="i-carbon-campsite" />
</RouterLink>
```

## Good

```vue
<a href="/settings">Settings</a>
<a href="https://github.com" aria-label="GitHub">
  <div class="i-carbon-logo-github" />
</a>
<a href="/docs"><img alt="Documentation" src="docs.png"></a>
<a href="https://example.com" title="Example">
  <span class="i-carbon-link" />
</a>
<RouterLink to="/">Home</RouterLink>
<RouterLink to="/" title="Home">
  <div class="i-carbon-campsite" />
</RouterLink>
```

## Limitations

Checked tags: `a`, `RouterLink` / `router-link`, `NuxtLink` / `nuxt-link`. Accessible content means non-whitespace text, interpolation, `v-text`/`v-html`, or a descendant `img`/`area` with a non-empty `alt`. Element-only children (icon wrappers) and `aria-hidden` subtrees do not count. `aria-label`, `aria-labelledby`, and a nonempty HTML `title` / `:title` (HTML-AAM fallback) name the control. An empty `title=""` does not. Named controls are not reported; there is no title-to-`aria-label` safe edit.

## Remediation

Add text content, an image with `alt`, or an `aria-label` / `aria-labelledby` binding.
