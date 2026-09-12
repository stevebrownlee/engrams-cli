---
trigger: glob
globs: frontend/**/*.ts,frontend/**/*.tsx
description: "Prohibit redundant conditional expressions and nested ternaries"
---


# Clean Expressions

## 1. Redundant boolean ternaries

Never map a boolean to its own literal. Pass the value directly.

```typescript
// BAD
disabled={isLoading ? true : false}
visible={isOpen ? true : false}

// GOOD
disabled={isLoading}
visible={isOpen}

// If coercion from a non-boolean is needed
<Checkbox checked={!!items.length} />
<Checkbox checked={Boolean(items.length)} />
```

## 2. Redundant value ternaries

Use `||`, `&&`, or `??` instead of a ternary that returns the same value it tests.

```typescript
// BAD
const label = name ? name : "Unknown";
const value = data !== null ? data : fallback;

// GOOD
const label = name || "Unknown";
const value = data ?? fallback;
```

## 3. Multi-state and reusable complex logic

When a prop has more than two possible values, extract the logic. Never use nested ternaries inline.

```typescript
// BAD - nested ternary inline in JSX
<Checkbox checked={allSelected ? true : someSelected ? undefined : false} />
<Badge variant={status === 'active' ? 'success' : status === 'pending' ? 'warning' : 'destructive'} />

// GOOD - named variable (when logic is used once)
const checkedState = allSelected || (someSelected ? undefined : false);
<Checkbox checked={checkedState} />

// GOOD - pure boolean version (when indeterminate is NOT needed)
const checkedState = allSelected || !someSelected;
<Checkbox checked={checkedState} />

// GOOD - lookup object (when mapping value → value)
const variantByStatus = { active: 'success', pending: 'warning', inactive: 'destructive' } as const;
<Badge variant={variantByStatus[status]} />
```

**Extract shared tokens from lookup maps.** When every entry in a `Record`/lookup shares a common class, prefix, or suffix, hoist the shared part out and keep only the varying token in the map. Compose at the call site with `cn()` or string concatenation. Repeating the shared token in every entry is structural duplication — changing the shared value means editing every row.

```typescript
// BAD - 'border-l-2' repeated in every non-empty entry
const borderStyles: Record<UrgencyLevel, string> = {
  overdue: 'border-l-2 border-l-destructive',
  today:   'border-l-2 border-l-amber-500',
  week:    'border-l-2 border-l-yellow-400',
  none:    '',
};
<Item className={borderStyles[urgency]} />

// GOOD - map only holds the varying color; compose at the call site
const urgencyBorderColor: Record<UrgencyLevel, string> = {
  overdue: 'border-l-destructive',
  today:   'border-l-amber-500',
  week:    'border-l-yellow-400',
  none:    '',
};
<Item className={cn(urgency !== 'none' && 'border-l-2', urgencyBorderColor[urgency])} />
```

## 4. Helper functions for reusable or complex logic

When logic has 3+ branches, or is reused across components, extract to a helper function with early returns or a switch.

```typescript
// Three-state checkbox: checked / indeterminate / unchecked
function getCheckedState(all: boolean, some: boolean) {
  if (all) return true;       // all items selected → checked
  if (some) return undefined;  // partial selection → indeterminate
  return false;                // nothing selected → unchecked
}

// Status → visual variant mapping
function getBadgeVariant(status: Status) {
  switch (status) {
    case 'active':  return 'success';
    case 'pending': return 'warning';
    case 'expired': return 'secondary';
    default:        return 'destructive';
  }
}

// Permission level → UI state
function getFieldAccess(role: Role, field: string) {
  if (role === 'admin') return 'edit';
  if (role === 'editor' && isEditableField(field)) return 'edit';
  if (role === 'viewer') return 'readonly';
  return 'hidden';
}
```

## 4. Prefer early returns over deeply nested conditionals

```typescript
// BAD
function process(item) {
  if (item) {
    if (item.active) {
      if (item.value > 0) {
        return item.value * 2;
      }
    }
  }
  return null;
}

// GOOD
function process(item) {
  if (!item || !item.active || item.value <= 0) return null;
  return item.value * 2;
}
```

## 5. Code clarity and top-level helpers

- Extract any non-trivial inline expression to a clearly named variable before using it in JSX.
- If a JSX prop value requires more than one operator to compute, it belongs in a variable above the return statement.
- Prop values in JSX must be immediately readable — no mental parsing required.
- When a function body, hook, or config callback contains multi-branch logic, extract it to a **named function at module scope** (top of the file, after imports). The function name documents intent and keeps the call site scannable.
- Same file is fine when the helper is only used once. If logic grows or is reused, move to a `*.utils.ts` or shared module following existing project layout.

```typescript
// BAD — multi-branch logic buried inside a component
function UserRow({ user }: Props) {
  let displayName: string;
  if (user.preferredName) {
    displayName = user.preferredName;
  } else if (user.firstName && user.lastName) {
    displayName = `${user.lastName}, ${user.firstName}`;
  } else {
    displayName = user.email ?? 'Unknown';
  }
  return <span>{displayName}</span>;
}

// GOOD — named helper at module scope, component stays scannable
function formatDisplayName(user: User): string {
  if (user.preferredName) return user.preferredName;
  if (user.firstName && user.lastName) return `${user.lastName}, ${user.firstName}`;
  return user.email ?? 'Unknown';
}

function UserRow({ user }: Props) {
  return <span>{formatDisplayName(user)}</span>;
}
```
