# Avoid automatic focus

This high-confidence recommended rule reports a concrete Vue correctness, reactivity, performance, or accessibility failure.

## Bad

```vue
<input autofocus>
```

## Good

```vue
<input>
```

## Limitations

Programmatic focus is outside this template-only rule. The safe fix covers only
boolean `autofocus`; valued forms remain visible for manual review because the
diagnostic name span does not cover the complete attribute.

## Remediation

Move focus only after an explicit user action when it is necessary.

Use `--fix-dry-run` to inspect the byte-range removal or `--fix-safe` to remove a
boolean `autofocus` and report the fresh post-fix scan.
