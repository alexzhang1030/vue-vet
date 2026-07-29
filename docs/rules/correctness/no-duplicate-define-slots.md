# `vue-vet/correctness/no-duplicate-define-slots`

Category: correctness  
Default severity: warning  
Confidence: high


This high-confidence rule reports more than one `defineSlots` call in `<script setup>`.

## Bad

```vue
defineSlots<{ default(): unknown }>()
defineSlots<{ footer(): unknown }>()
```

## Good

```vue
defineSlots<{ default(): unknown; footer(): unknown }>()
```

## Remediation

Describe every slot in one type declaration.

