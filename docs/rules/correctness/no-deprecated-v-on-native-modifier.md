# Remove Vue 2 native event modifiers

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<Widget @click.native="activate" />
```

## Good

```vue
<Widget @click="activate" />
```

## Limitations

Targets only the literal `.native` modifier.

## Remediation

Declare child emits correctly and rely on Vue 3 listener fallthrough.

