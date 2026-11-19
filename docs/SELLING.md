# Selling the NoDAW plugins (Gumroad + 14-day trial)

The trial and activation are built in (`crates/nodaw-kit/src/license.rs`): 14 days full-featured
from first launch, then a Gumroad license key unlocks it. Unlicensed copies keep working after the
trial but go silent for 0.5 s every 45 s. Activation calls
`POST https://api.gumroad.com/v2/licenses/verify` with `product_id` + `license_key`, so each
plugin needs **its own Gumroad product with license keys turned on**.

## Gumroad checklist

For each row: Gumroad > Products > New product > **Digital product**, then

1. Name and URL slug as below (the slug is what the plugin's Buy button opens; if you pick a
   different slug, change the URL in `gumroad.rs` too).
2. Price as below (suggested; one-time, not a membership).
3. Content tab: upload the Windows zip from the GitHub release (or link the release page).
4. Content tab: turn on **"Generate a unique license key per sale"**. Gumroad then shows the
   product's `product_id` (in the sample verification request). Copy it.
5. Publish when ready.

| Product name | Slug (Buy URL) | Suggested price | ID goes in |
|---|---|---|---|
| NoDAW Screw | `nodaw-screw` | $9 | `SCREW_ID` / `NODAW_GUMROAD_ID_SCREW` |
| NoDAW Racks | `nodaw-racks` | $19 | `RACKS_ID` / `NODAW_GUMROAD_ID_RACKS` |
| NoDAW Harmonic Space | `nodaw-harmonic-space` | $19 | `HARMONIC_SPACE_ID` / `NODAW_GUMROAD_ID_HARMONIC_SPACE` |
| NoDAW Chronos Dynamic EQ | `nodaw-chronos-dynamic-eq` | $19 | `CHRONOS_DYNAMIC_EQ_ID` / `NODAW_GUMROAD_ID_CHRONOS_DYNAMIC_EQ` |
| NoDAW Temporal Pitch Portal | `nodaw-temporal-pitch-portal` | $15 | `TEMPORAL_PITCH_PORTAL_ID` / `NODAW_GUMROAD_ID_TEMPORAL_PITCH_PORTAL` |
| NoDAW Chronos Weaver | `nodaw-chronos-weaver` | $15 | `CHRONOS_WEAVER_ID` / `NODAW_GUMROAD_ID_CHRONOS_WEAVER` |
| NoDAW Plugin Bundle (all six) | `nodaw-plugin-bundle` | $49 | none: a Gumroad *bundle* of the six products above |

Bundle: create it with Gumroad's bundle feature from the six products, so each buyer gets each
product (and its own license key). Check one test purchase shows six keys before launch.

## Dropping the IDs in (one small change)

Either edit the six `TODO_…` strings in `crates/nodaw-kit/src/gumroad.rs`, or (no commit needed)
add repository **variables** `NODAW_GUMROAD_ID_RACKS` etc. under Settings > Secrets and variables >
Actions > Variables; the Windows CI build passes them to the compiler. Then tag a new release.

## Before taking money

* Do one real purchase (Gumroad lets you buy your own product with a 100% discount code) and
  activate it in a real DAW.
* The Windows binaries are **not code-signed**: Windows SmartScreen will warn on the installer.
* macOS is not built yet.
