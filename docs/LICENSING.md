# Trial and activation

NoDAW Screw runs as a full **14-day trial** from its first launch on a computer. After that, an
unlicensed copy keeps working: audio is processed, presets and sessions save and load, nothing is
locked. The only difference is a **0.5 s dropout to silence every 45 s** (15 ms raised-cosine fades,
so it never clicks). Bypassed audio is left alone. Activating with a Gumroad license key removes it.

## For the owner: product IDs before selling

Every plugin's Gumroad `product_id` and Buy URL are in `crates/nodaw-kit/src/gumroad.rs` (or the
`NODAW_GUMROAD_ID_*` / `NODAW_GUMROAD_URL_*` build-time variables). Where to find the ID and what to
create on Gumroad: `docs/SELLING.md`. Until an ID is set, the dialog says activation opens when the
plugin goes on sale.

## How it works

* **State file** `%APPDATA%\NoDAW Labs\Screw\license.json` (macOS `~/Library/Application Support/NoDAW Labs/Screw/`, Linux `$XDG_CONFIG_HOME/NoDAW Labs/Screw/`):
  trial start, latest time seen, license key, last verification time, buyer email, and an HMAC-SHA256 over all of it.
  A second signed copy of the trial start sits in `%LOCALAPPDATA%\NoDAW Labs\Screw\.cache`, so deleting the main file does not restart the trial.
  Editing the file by hand ends the trial ("tampered"); activating fixes it. Winding the clock back does not extend the trial (`last_seen`).
  This is anti-casual protection, not DRM.
* **Unwritable location**: never an error. The state then lives in memory (a fresh trial for that session).
* **Activation**: the page posts the key to the plugin's controller; a background thread calls
  `POST https://api.gumroad.com/v2/licenses/verify` (`product_id`, `license_key`, `increment_uses_count=false`, 12 s timeout, rustls).
  Never on the audio thread, never blocking the UI thread; the page polls `license` for the result.
* **Licensed offline**: a licensed copy is never locked out. After 30 days it re-checks in the background when the plugin is opened;
  only a definitive answer from Gumroad (unknown key, refunded, charged back, subscription ended) removes the license. No network, timeouts or server errors change nothing.
* **Audio thread**: reads one atomic flag per block; the dropout is a precomputed sample clock (no allocation, no locks).
* **Buy button**: opens `BUY_URL` in the default browser (`explorer.exe <url>` on Windows, `open` on macOS).

## Tests

`cargo test -p nodaw-screw --no-default-features license` covers days left, expiry, clock rollback, the signed record and tamper detection,
the shadow copy surviving deletion, an unwritable location, licensed-offline (a year without network), the 30-day recheck,
Gumroad response parsing (valid, unknown key, refunded, disputed, subscription ended, server errors), activation and re-locking, and the dropout
(exactly 0.5 s of silence after 45 s, no step larger than the fade allows, untouched when off).

UI screenshots (mocked state): `docs/screenshots/16-*.png` … `22-*.png`.
