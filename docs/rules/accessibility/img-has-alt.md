# Require image alternatives

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<img src="avatar.png">
```

## Good

```vue
<img src="avatar.png" alt="Account avatar">
```

## Limitations

The rule cannot decide whether an image is decorative; `alt=""` is accepted.

## Remediation

Describe meaningful images or use an empty alt for decorative images.

