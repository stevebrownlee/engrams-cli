# Theming — choose the metaphor by mechanics, not habit

The theme is an **explanatory instrument**, not decoration. A well-chosen metaphor lets the learner *predict behavior they haven't been taught yet* ("if the cache is a pantry, then… stuff must go stale? — yes, that's TTL"). A badly-chosen one is wallpaper. And a repeated one is worse than wallpaper: **do not reuse a theme from a previous module or from the examples in this file because it's comfortable.** The concept decides the theme, every time, fresh.

## The selection procedure

1. **Write the topic's core mechanics as plain verbs.** Ignore the domain vocabulary; describe what *happens*: things flow, things expire, things queue, things get checked at gates, one thing splits into many, copies drift apart, pressure builds, a switch flips, a record is kept.
2. **Brainstorm three real-world systems with the same verbs.** Everyday, physical, familiar — places the learner has stood in. If the topic is "traffic gradually shifts from an old version to a new one with a fast way back," the real world offers highway lane closures, contraflow switches, and on-ramp metering before it offers anything aviation-related.
3. **Score the candidates:**
   - *Coverage* — does every key term get a natural slot? (If three core concepts have no home in the metaphor, it's too small.)
   - *Predictive power* — does the metaphor's normal behavior imply the system's normal behavior? Can the learner reason forward with it?
   - *Audience familiarity* — a metaphor the learner has never experienced explains nothing.
   - *Visual buildability* — can its world be drawn convincingly in CSS/SVG (lanes, shelves, envelopes, gauges)?
   - *Tone fit* — a playful diner suits a dev-tools course; a healthcare compliance module may want something steadier.
4. **Stress-test with the hardest concept.** Take the module's most confusing idea and force it through the metaphor. If it needs contortions ("well, imagine the pantry had a… sub-pantry"), discard the candidate and try the next.
5. **Name where it breaks — on purpose.** Every metaphor lies eventually. Find the lie before the learner does, and say it in the module ("unlike letters, messages can be delivered twice — this is where the mail metaphor stops"). Naming the breakdown is a teaching moment and builds trust.

**On name-gifts:** a repo or product name sometimes gifts a theme (a repo called `control_tower` → air-traffic control). Take the gift **only if it survives the same scoring** — it worked for observability because observability genuinely *is* surveillance of many moving things reporting to a central screen. If the name-gift fits the mechanics poorly, decline it; the concept outranks the pun.

## Worked examples — concept → metaphor → why it maps → where it breaks

Use these as *calibration for the kind of mapping to find* — not as a menu to pick from.

| Concept | Metaphor | Why the mechanics map | Where it breaks (say so) |
|---|---|---|---|
| Deployment strategies (blue-green, canary, rollback) | **Highway traffic management** | Two parallel roadways = two environments; contraflow switch = blue-green cutover; metering a few cars onto the new lane and watching for crashes = canary + metrics; reopening the old lane = rollback | Cars are individuals who notice the switch; users shouldn't. Software "lanes" cost money while idle |
| Caching + TTL + invalidation | **Prep counter / pantry vs. the warehouse run** | Nearby shelf = cache; the long warehouse trip = the database query; freshness stickers = TTL; tossing a dish when the recipe changes = invalidation; everyone cooking the same missing dish at once = stampede | Food costs money to discard; cache entries are free to drop — deletion is *safer* than in the kitchen |
| Message queues / async processing | **Postal sorting facility** | Envelopes = messages; drop-off = enqueue (sender leaves!); sorting bins = topics/partitions; a returned letter = dead-letter queue; certified mail = acknowledgment | Real mail is delivered at most once; queues often deliver *at least* once — duplicates are normal |
| Authentication vs. authorization | **Hotel front desk + keycard** | Showing ID at check-in = authn (who are you); which doors the keycard opens = authz (what may you do); keycard expiry = session/token TTL; revoking a card without re-checking ID = token revocation | The hotel checks ID once; systems re-verify every request — the keycard is shown at *every door* |
| Database indexes | **A book's index** | Scanning every page = full table scan; the alphabetized index at the back = B-tree lookup; an index nobody consults but that must be updated on every edit = write overhead of unused indexes | A book has one reader at a time; databases juggle thousands — contention has no page-flipping analog |
| Distributed tracing | **A tracked parcel's journey** | One tracking number across every depot = trace ID; each scan event with timestamp = span; the depot that held it two days = the slow span; handing the number to the next carrier = context propagation | Parcels take one path; a request fans out to parallel calls — the "journey" can be a tree, not a line |
| Rate limiting / backpressure | **Nightclub door with a capacity counter** | Bouncer's clicker = token bucket; line outside = queue/backpressure; "one out, one in" = steady-state; VIP line = priority tiers; fire-code limit = hard cap protecting everyone inside | The club turns people away rudely; good APIs return Retry-After — the refusal itself carries information |
| Distributed transactions / sagas | **Booking a trip (flight + hotel + car)** | Each booking = a step in the saga; free-cancellation windows = compensating actions; hotel full after flight booked = mid-saga failure → cancel the flight = compensation; nonrefundable fare = a step that can't compensate | Travel sites make this feel instant; sagas are visibly eventual — the learner should expect in-between states |
| Event sourcing / append-only logs | **An accountant's ledger** | Never erase — append a correcting entry = immutable events; the balance = current state derived by replay; auditing old pages = time travel/debugging; monthly closing totals = snapshots | Ledgers have one writer; event logs have many — ordering across writers is its own hard problem |
| Garbage collection / memory | **Restaurant table bussing** | Diners leave, table still dirty = unreferenced memory; busser sweep = GC pause; a party that never leaves = a leak; the sawtooth of full-then-cleared tables = normal heap behavior | The busser can see who left; GC must *prove* nothing references the table — that proof is the expensive part |
| Load balancing | **Supermarket checkout lanes** | One queue feeding many registers = round-robin/least-busy; a register closing mid-shift = instance failure + drain; express lane = routing by request class; every register needing the same price database = shared state | Shoppers pick their own lane and regret it; balancers reassign instantly and shoppers (requests) don't mind |
| Feature flags | **Circuit-breaker panel in a house** | Each breaker = one flag; flipping without rewiring = decoupling deploy from release; the master switch = kill switch; a breaker left on for years that nobody remembers = stale-flag debt | House breakers are binary and local; flags can be percentage-based and per-user — the panel has no "10% of the kitchen" |

## After choosing: let the metaphor drive the craft

- **Vocabulary discipline**: introduce metaphor and real term together ("a span — one stopwatch lap"), quiz on the real term, and keep one consistent mapping table in your head (lane = environment, *always* — never sometimes lane = version).
- **Visual identity follows the world**: palette, fonts, and the hero's signature animated element all come from the metaphor's physical world — highway signage greens and lane-dash animations; kraft-paper and rubber stamps for postal; ledger cream and rule-lines for accounting. This is what makes each module feel designed rather than templated.
- **The Big Picture page is the metaphor's payoff**: if the metaphor was chosen well, the whole system fits in one drawing of that world with the real terms pinned on. If you can't draw the infographic in the metaphor's world, you chose the wrong metaphor — better to learn that in planning than in section nine.
