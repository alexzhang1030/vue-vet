# Require labels to associate with a control

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<label>Email</label>
<input id="email" type="email">
```

## Good

```vue
<label for="email">Email</label>
<input id="email" type="email">

<label>
  Email
  <input type="email">
</label>
```

## Limitations

Accepts a static or bound `for` attribute, or a nested labelable control (`input`, `textarea`, `select`, `button`, `meter`, `output`, `progress`). Matching `for` to an `id` across the tree is not validated.

## Remediation

Point `for` at the control `id`, or nest the control inside the label.
