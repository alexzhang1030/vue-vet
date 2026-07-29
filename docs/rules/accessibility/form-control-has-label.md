# Require labels on form controls

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<input type="text" name="email">
<textarea />
```

## Good

```vue
<label for="email">Email</label>
<input id="email" type="text">

<label>
  Notes
  <textarea />
</label>

<input aria-label="Search" type="search">
<input type="hidden" name="csrf" value="token">
```

## Limitations

Checks `input` (except `hidden` / `button` / `submit` / `reset` / `image`), `textarea`, `select`, `meter`, `output`, and `progress`. Association is accepted via a `<label>` ancestor, matching static or identically bound `for`/`id` tokens, or `aria-label` / `aria-labelledby`. Cross-file label/control pairs are not joined.

## Remediation

Nest the control in a label, wire `for`/`id`, or add an accessible name.
