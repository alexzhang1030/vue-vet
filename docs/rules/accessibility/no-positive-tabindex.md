# Avoid positive tabindex

Positive tabindex values override document order and make keyboard navigation unpredictable.

## Bad

```vue
<div tabindex="3">Open</div>
```

## Good

```vue
<button type="button">Open</button>
<div tabindex="0">Custom control</div>
```

## Limitations

Dynamic `:tabindex` expressions are not evaluated.

## Remediation

Prefer native controls and natural document order.
