# Avoid obsolete distracting elements

The `<blink>` and `<marquee>` elements are obsolete and can create inaccessible motion.

## Bad

```vue
<marquee>Breaking news</marquee>
```

## Good

```vue
<p>Breaking news</p>
```

## Remediation

Use semantic content and CSS that respects `prefers-reduced-motion`.
