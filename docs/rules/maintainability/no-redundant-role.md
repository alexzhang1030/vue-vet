# Prefer native semantics without redundant roles

Adding the same role an HTML element already owns creates noise and can drift from native behavior.

## Bad

```vue
<button role="button">Save</button>
```

## Good

```vue
<button>Save</button>
<button role="switch" aria-checked="false">Notifications</button>
```

## Remediation

Remove roles that exactly duplicate native semantics.
