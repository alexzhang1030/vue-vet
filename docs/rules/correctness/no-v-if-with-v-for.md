# Avoid v-if with v-for on one element

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<li v-for="item in items" v-if="item.visible" :key="item.id" />
```

## Good

```vue
<template v-for="item in items" :key="item.id"><li v-if="item.visible" /></template>
```

## Limitations

This rule checks directive co-location and does not evaluate conditions.

## Remediation

Move the condition to a wrapper or filter the collection.

