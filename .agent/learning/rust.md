# Rust Learning Log

## 2026-07-14 - Unit Testing Active Wars and Combat Resolution
**Learning:** In the politics phase, resolving active wars triggers simulated combat if both sides have units with positive strength in the same theater. Combat damage is proportional and reduces the loser's units' strength. If a unit's strength falls below 5.0, it is destroyed, which can cause the theater to become uncontested and end the war. Additionally, if the total strength losses in a single combat exceed the `WAR_EXHAUSTION_THRESHOLD` (500.0), the war immediately concludes due to exhaustion.
**Action:** When writing unit tests where a war must remain active after running `resolve_active_wars`, ensure the units have enough initial strength to survive combat without being destroyed, but low enough strength (e.g. 50.0) so that total combat damage does not exceed `WAR_EXHAUSTION_THRESHOLD`.
