# `vue-vet/accessibility/no-distracting-elements`

Category: accessibility  
Default severity: warning  
Confidence: high

Blinking / marquee-style elements are distracting and are blocked by modern accessibility guidance.

## Bad

```vue
<template>
  <marquee>news</marquee>
</template>
```

## Good

```vue
<template>
  <p>news</p>
</template>
```

## Remediation

Use static content or CSS animations that respect `prefers-reduced-motion`.
