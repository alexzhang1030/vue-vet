# `vue-vet/security/no-v-html`

Category: security  
Default severity: warning  
Confidence: high

`v-html` assigns a string to an element's HTML content. If that string contains
untrusted input, the browser interprets it as markup and script-capable content
rather than displaying it as text.

## Bad

```vue
<template>
  <article v-html="comment.body" />
</template>
```

## Good

Prefer normal interpolation when the content is text:

```vue
<template>
  <article>{{ comment.body }}</article>
</template>
```

If raw HTML is a product requirement, sanitize it at the trust boundary before
it reaches the component and document the sanitizer and allowed markup. Vue Vet
still reports the directive because it cannot prove an application-specific
sanitization contract from the template alone.

## Detection

The rule reads Vize directive nodes from the Vue template AST. Comments, text,
script strings, `data-v-html`, and similarly named custom directives are not
findings. The diagnostic highlights the directive name at its exact byte range.
