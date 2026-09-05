---
name: frontend-design
description: Use when designing a new frontend component, page, or feature, or refining and refactoring an existing component's architecture, composition, state management, or styling
---

# Frontend Design & Component Refinement

Guide for architecting new frontend components and pages, or systematically refining existing frontend code to meet production architectural, compositional, and stylistic standards.

---

## Overview

High-quality frontend development enforces a strict separation of concerns across three distinct tiers:
1. **Data Tier:** API transport, query hooks, typed contracts, and cache management.
2. **Provider/Orchestration Tier:** State orchestration, mutations, business logic, derived calculations, and navigation.
3. **View/Presentation Tier:** Pure declarative UI composition using design system primitives.

Whether creating a new component or refining an existing one, logic must never leak into views, transport details must never reach presentation, and UI chrome must remain consistent across the application.

---

## When to Use

### Designing a New Component or Feature
- Building a new page, module, workspace, modal, or complex UI widget.
- Establishing new API data flows and state orchestration.
- Composing multiple interactive views with shared context.

### Refining an Existing Component
- A component has grown unwieldy with mixed data fetching, local state, and JSX rendering.
- Manual memoization (`useMemo`, `useCallback`) clutter is obscuring component intent.
- Modal state is handled via local booleans rather than the central modal registry.
- Unstyled or raw HTML elements need modernization to shared Design System primitives.
- Async loading/error states block the entire page rather than the scoped region.
- User-facing text is hardcoded rather than localized via `useTranslation`.

---

## Architecture: The 3-Tier Layer Model

```
┌─────────────────────────────────────────────────────────┐
│                     View / Page Tier                    │
│   (Declarative JSX, Design System Layout, Local UI)     │
└────────────────────────────▲────────────────────────────┘
                             │ consumes state & handlers
┌────────────────────────────┴────────────────────────────┐
│              Provider / Orchestration Tier              │
│  (Custom Provider Hook, Workflow State, Action Handlers)│
└────────────────────────────▲────────────────────────────┘
                             │ queries & triggers mutations
┌────────────────────────────┴────────────────────────────┐
│                        Data Tier                        │
│   (useRequestQuery, useRequestMutation, Query Keys)     │
└─────────────────────────────────────────────────────────┘
```

### 1. Data Layer (`*.api.ts`, `*.keys.ts`, `*.types.ts`)
- **Query & Mutation Hooks:** Use framework request wrappers (`useRequestQuery`, `useRequestMutation`). Never call raw fetch/Axios directly in components.
- **Stable Query Key Factories:** Organize keys hierarchically to enable targeted cache invalidation:
  ```typescript
  export const featureKeys = {
    all: ['feature'] as const,
    lists: () => [...featureKeys.all, 'list'] as const,
    list: (filters: FeatureFilters) => [...featureKeys.lists(), filters] as const,
    details: () => [...featureKeys.all, 'detail'] as const,
    detail: (id: string) => [...featureKeys.details(), id] as const,
  };
  ```
- **Colocated Cache Invalidation:** Invalidate relevant query keys in `onSuccess` handlers directly within or adjacent to the mutation hook.
- **Structured Parameters:** Pass query parameters using the `params` config object; never manually concatenate URL query strings.
- **Typed Contracts:** Rely on generated OpenAPI/schema types for request and response envelopes. Separate UI/domain models into `*.types.ts` when needed. Never manually edit generated API files.

### 2. Provider / Orchestration Layer (`use<Feature>Provider.ts`)
- **Single Source of Truth:** Manages active filters, selection, modals, derived calculations, mutation dispatch, and route navigation.
- **Shield the View:** The view receives clean data objects and explicit callbacks (e.g., `onSelectItem`, `onApplyFilter`), never raw API response envelopes or query dispatchers.
- **Decomposition Pattern for Complex Workspaces:** When a provider mixes materially different concerns (e.g., remote syncing, multi-step actions, complex local UI state), decompose into focused sub-hooks composed by the master provider:
  - `use<Feature>Data`: Fetches and normalizes queries, handles pagination and filter state.
  - `use<Feature>Actions`: Implements user interactions, triggers mutations, shows toasts/feedback.
  - `use<Feature>UiState`: Manages transient panel tabs, expanded rows, drawer states.
  - `use<Feature>Provider`: Composes the above hooks into a unified context value.
- **Avoid Trivial Over-abstraction:** Do not create 2-line single-use helper hooks merely for symmetry.

### 3. Page / View Layer (`<Feature>Page.tsx`, `<Feature>Component.tsx`)
- **Declarative Presentation:** Focuses exclusively on rendering JSX and user event delegation.
- **Region Decomposition:** Extract substantial page sections (e.g., summary header, filter bar, data table, detail drawer) into dedicated sibling components.
- **Component Translation Ownership:** Components call `useTranslation()` directly. Avoid passing `t` functions down through component props.
- **Scoped Status Boundaries:** Scope loading and error displays (e.g., `OperationStateDisplay`) to the specific content region being updated, ensuring search bars, headers, and navigation stay mounted and interactive.

---

## Component Design & Composition Patterns

### Layout with Design System Primitives
Always prefer shared layout and typography primitives over raw `div` tags, inline styles, or third-party primitives:
- Use `Stack`, `Column`, `Row`, and `Box` for spacing and flex layouts.
- Use `Typography` variants (`h1`, `h2`, `body`, `caption`) for consistent font hierarchies.
- Use core UI components (`Button`, `Badge`, `Card`, `InfoCard`, `DataTable`, `DropdownMenu`).

### React Compiler & Optimization
- **Directive:** Add `'use memo';` at the top of all React component files and custom hook files when the React Compiler is active.
- **Do NOT add `'use memo';`** to type files, constants, query key definitions, configuration files, barrels, or pure utility functions.
- **Avoid Manual Memoization:** Do not clutter components with manual `useMemo` and `useCallback` wrappers unless benchmarked for reference stability requirements (such as third-party virtualized lists or canvas integrations).

### Responsive Search & Filtering
- Synchronize server-backed search with immediate responsive typing:
  1. Maintain immediate local state for the input field (`const [searchTerm, setSearchTerm] = useState('')`).
  2. Debounce the committed filter/URL parameter update (e.g., 300ms debounce before triggering query refetch).
  3. Keep the input responsive while queries update in the background.

### Multi-Item Selection & Batch Controls
- Handle all three checkbox states correctly: **checked** (all selected), **unchecked** (none selected), and **indeterminate** (subset selected).
- Update action bar labels dynamically with exact counts (e.g., "3 items selected").
- Provide clear "Select All" and "Clear Selection" affordances.

### Discriminated Unions & Exhaustiveness
- When rendering status badges, icon variants, or step wizards driven by enums or discriminated union types, implement exhaustive switch/case statements:
  ```typescript
  switch (status.type) {
    case 'active':
      return <Badge variant="success">{t('status.active')}</Badge>;
    case 'pending':
      return <Badge variant="warning">{t('status.pending')}</Badge>;
    case 'archived':
      return <Badge variant="neutral">{t('status.archived')}</Badge>;
    default: {
      const _exhaustive: never = status;
      return null;
    }
  }
  ```

### Internationalization (i18n)
- Every user-visible string must use `t('key')` via `useTranslation()`.
- Never hardcode raw English text in JSX, placeholders, aria-labels, button titles, or toast alerts.
- Handle pluralization using i18n plural keys (`key_one`, `key_other`) rather than ternary string concatenations.

---

## Modals and Dialog Architecture

Follow a standardized, typed modal registration and presentation lifecycle:

```
┌────────────────────────────────┐
│         Caller / Hook          │
│ const res = await openModal(…) │
└───────────────┬────────────────┘
                │ opens with typed props & awaits Promise<TResult>
┌───────────────▼────────────────┐
│         Modal Wrapper          │
│   implements ModalComponentProps│
│   composes ConfirmDialogLayout │
└───────────────┬────────────────┘
                │ resolves promise on confirm/cancel
┌───────────────▼────────────────┐
│   Typed Result to Caller       │
│   { confirmed: true, data }    │
└────────────────────────────────┘
```

1. **Central Registration:** Register the modal in the application modal registry with a stable string ID constant.
2. **Typed Props & Results:** Implement `ModalComponentProps<TResult>`:
   ```typescript
   export interface ConfirmActionModalProps extends ModalComponentProps<ConfirmActionResult> {
     itemId: string;
     itemName: string;
   }

   export type ConfirmActionResult =
     | { confirmed: true; reason: string }
     | { confirmed: false };
   ```
3. **Imperative Invocation:** Open modals via the shell modal hook (`const { openModal } = useAppShell(); const result = await openModal(CONFIRM_MODAL_ID, { itemId, itemName });`).
4. **Shared Dialog Chrome:** Always compose `ConfirmDialogLayout` (or the design system's dialog container). Never construct custom modal header, body, footer, and close button wrappers from raw HTML.
5. **Encapsulated Internal State:** Form inputs, validation errors, and step indices belong inside the modal component.
6. **Mutation Ownership:**
   - The caller/provider executes the mutation upon receiving `{ confirmed: true }`, unless keeping the modal open during mutation failure for user retry is an explicit UX requirement.
   - Do NOT use local boolean state (`const [isModalOpen, setIsModalOpen] = useState(false)`) to toggle registered application modals.

---

## Step-by-Step Workflow

### Mode A: Designing a New Component / Feature

1. **Clarify Requirements & User Outcomes:**
   - Identify inputs, interactive triggers, asynchronous flows, and observable outputs.
2. **Define Data Contracts:**
   - Define query keys, OpenAPI types, request parameters, and mutation signatures.
3. **Build the Provider / Custom Hook:**
   - Wire up data fetching, local state (search, filters, selections), and action handlers.
4. **Construct Layout with Design System:**
   - Compose views using `Column`, `Row`, `Stack`, `Typography`, and domain components.
5. **Implement Edge States:**
   - Add empty states, error boundaries, loading skeletons, and localized strings.
6. **Verify & Test:**
   - Run type checks, linting, and behavioral tests.

### Mode B: Refining an Existing Component

1. **Audit Existing Component Smells:**
   - Check for direct `fetch`/Axios calls inside JSX.
   - Check for bloated state (10+ `useState` calls in a single component).
   - Check for raw HTML tags (`<div className="flex...">`) that should be Design System primitives (`<Row gap={4}>`).
   - Check for hardcoded strings and missing `t()` localization.
   - Check for manual `useMemo`/`useCallback` pollution.
2. **Extract the Data Layer:**
   - Move queries and mutations into dedicated hooks with stable query key factories.
3. **Separate Orchestration from Presentation:**
   - Move state management, derived values, and handlers into `use<Component>Provider` (or focused sub-hooks if complex).
4. **Refactor JSX to Design System Primitives:**
   - Replace unstructured CSS/div hierarchies with `Stack`, `Column`, `Row`, and semantic Design System components.
5. **Scope Async & Error Boundaries:**
   - Ensure loading and error indicators only cover the affected region.
6. **Verify Observable Behavior:**
   - Confirm all existing functionality works with zero regression.

---

## Simplicity & Code Cleanliness Guardrails

- **Copy Principles, Not Incidental Syntax:** Adapt the architecture without copying outdated boilerplate, commented-out code, or redundant aliases.
- **Match Granularity to Complexity:** Do not fragment a simple 80-line component into five files. Decompose only when distinct concerns or reuse justify it.
- **No Speculative Abstractions:** Do not add generic config objects, unused prop parameters, or extensible plugin hooks until an actual requirement demands them.
- **Eliminate Narrator Comments:** Code should be self-documenting through clear naming. Remove comments that merely narrate what the next line of code does.
- **Eliminate Unsafe Casts:** Avoid `as unknown as Foo` or `as any`. Leverage proper TypeScript type guards and narrowing.

---

## Testing Observable Contracts

State every test's observable contract before writing it:
> *"When [user input or condition occurs], [the user or caller] must observe [concrete outcome]."*

```typescript
// ✅ GOOD: Tests observable user-facing behavior and state transitions
test('submitting confirmation modal triggers mutation and displays success notification', async () => {
  render(<FeatureWorkspace />);
  await userEvent.click(screen.getByRole('button', { name: /archive item/i }));
  
  const modal = screen.getByRole('dialog');
  await userEvent.type(within(modal).getByLabelText(/reason/i), 'Duplicate record');
  await userEvent.click(within(modal).getByRole('button', { name: /confirm/i }));

  expect(await screen.findByText(/item archived successfully/i)).toBeInTheDocument();
});

// ❌ BAD: Testing internal state, component mounting boilerplate, or static strings
test('renders without crashing', () => {
  render(<FeatureWorkspace />);
});
```

- Prefer integration tests covering provider + view interactions over testing trivial presentational components in isolation.
- Focus tests on state transitions, validation boundaries, error handling, and permission constraints.

---

## Common Pitfalls & Rationalizations

| Excuse / Rationalization | Reality | Fix |
|---|---|---|
| *"Putting data fetching directly in the component is faster for small components."* | Leaks transport details into views, making testing and refactoring difficult. | Use a dedicated query hook and pass data via a provider or props. |
| *"I'll use local boolean `useState` to toggle the modal for now."* | Bypasses the application modal stack, breaks keyboard trap/escape handling, and prevents clean async return values. | Register the modal and open via `openModal(MODAL_ID, props)`. |
| *"I need manual `useMemo` and `useCallback` everywhere for performance."* | The React Compiler handles memoization automatically with `'use memo';`. Manual memoization adds visual noise and stale dependency bugs. | Add `'use memo';` at top of file and let the compiler optimize. |
| *"Passing `t` down through props avoids importing `useTranslation` in child components."* | Creates prop drilling and couples component signatures to translation implementation. | Call `useTranslation()` directly inside the component that renders the text. |
| *"I'll wrap the entire page in `isLoading ? <Spinner /> : <Content />`."* | Unmounts page headers, filter bars, and search inputs during background refreshes, creating jarring UI layout shifts. | Scope `OperationStateDisplay` or loading skeletons to the specific data grid or panel region. |
| *"Raw `<div>` with utility classes is easier than Design System primitives."* | Breaks design consistency, theme updates, and responsive spacing standards across the platform. | Use `Stack`, `Column`, `Row`, `Typography`, and shared Design System components. |

---

## Final Verification Checklist

Before finalizing any new or refined component:

- [ ] **3-Tier Separation:** Data fetching is isolated in API hooks; business logic lives in a provider/custom hook; view is purely declarative.
- [ ] **Design System:** Layout utilizes `Column`, `Row`, `Stack`, `Typography`, and design system components.
- [ ] **React Compiler:** `'use memo';` is present on all component and custom hook files (and absent from type/constant/key files).
- [ ] **Modal Standards:** Modals are registered, implement `ModalComponentProps`, compose `ConfirmDialogLayout`, and return typed results.
- [ ] **Localization (i18n):** Zero hardcoded user-facing strings; all copy uses `t()` with keys in locale files.
- [ ] **Search & Selection UX:** Server search has debounced query commits; multi-selection supports indeterminate tri-state.
- [ ] **Scoped Loading States:** Async loaders are scoped to their specific content region without unmounting surrounding controls.
- [ ] **Type Safety:** Discriminated unions are handled exhaustively with zero unsafe `any` or loose `as` casts.
- [ ] **Clean Code:** No narrator comments, dead code, or premature abstractions.
- [ ] **Lint & Typecheck:** Passes `npm run lint:quiet` and `npm run type-check`.
