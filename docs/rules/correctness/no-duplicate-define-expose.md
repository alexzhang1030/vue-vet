# `vue-vet/correctness/no-duplicate-define-expose`

Category: correctness  
Default severity: warning  
Confidence: high


This high-confidence rule reports multiple `defineExpose` calls in one `<script setup>` block.

## Bad

```vue
defineExpose({ open })
defineExpose({ close })
```

## Good

```vue
defineExpose({ open, close })
```

## Remediation

Expose the public component surface in one object.

