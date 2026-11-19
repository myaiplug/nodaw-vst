//! Gumroad product IDs and Buy pages for every NoDAW plugin, in one place.
//!
//! To turn on activation for a plugin, do ONE of these (no other code changes):
//!
//! 1. Paste the `product_id` into the table below (Gumroad dashboard: open the product >
//!    Content > License key section; the ID is shown in the sample `curl` there), or
//! 2. Set the build-time environment variable named next to it (for example
//!    `NODAW_GUMROAD_ID_RACKS`) — CI reads these from the repository's Actions *variables*
//!    (Settings > Secrets and variables > Actions > Variables), so a release needs no commit.
//!
//! The Buy URL works the same way (`NODAW_GUMROAD_URL_<PLUGIN>`). While an ID still starts with
//! `TODO`, the plugin's license dialog says activation is not available yet; the 14-day trial
//! and the post-trial dropouts work regardless.
//!
//! Product IDs are not secret (Gumroad's verify API only answers "is this key valid for this
//! product"), so committing them is fine.

/// `option_env!` with a fallback, usable in a `static`.
macro_rules! cfg_or {
    ($var:literal, $default:expr) => {
        match option_env!($var) {
            // An empty variable (an unset Actions variable expands to "") keeps the default.
            Some(v) if !v.is_empty() => v,
            _ => $default,
        }
    };
}

// ── product IDs ─────────────────────────────────────────────────────────────────────────────
pub const SCREW_ID: &str = cfg_or!("NODAW_GUMROAD_ID_SCREW", "TODO_GUMROAD_PRODUCT_ID");
pub const RACKS_ID: &str = cfg_or!("NODAW_GUMROAD_ID_RACKS", "TODO_GUMROAD_PRODUCT_ID_RACKS");
pub const HARMONIC_SPACE_ID: &str =
    cfg_or!("NODAW_GUMROAD_ID_HARMONIC_SPACE", "TODO_GUMROAD_PRODUCT_ID_HARMONIC_SPACE");
pub const CHRONOS_DYNAMIC_EQ_ID: &str =
    cfg_or!("NODAW_GUMROAD_ID_CHRONOS_DYNAMIC_EQ", "TODO_GUMROAD_PRODUCT_ID_CHRONOS_DYNAMIC_EQ");
pub const TEMPORAL_PITCH_PORTAL_ID: &str = cfg_or!(
    "NODAW_GUMROAD_ID_TEMPORAL_PITCH_PORTAL",
    "TODO_GUMROAD_PRODUCT_ID_TEMPORAL_PITCH_PORTAL"
);
pub const CHRONOS_WEAVER_ID: &str =
    cfg_or!("NODAW_GUMROAD_ID_CHRONOS_WEAVER", "TODO_GUMROAD_PRODUCT_ID_CHRONOS_WEAVER");

// ── Buy pages (the plugin's Buy button) ───────────────────────────────────────────────────────
pub const SCREW_URL: &str =
    cfg_or!("NODAW_GUMROAD_URL_SCREW", "https://nodawlabs.gumroad.com/l/nodaw-screw");
pub const RACKS_URL: &str =
    cfg_or!("NODAW_GUMROAD_URL_RACKS", "https://nodawlabs.gumroad.com/l/nodaw-racks");
pub const HARMONIC_SPACE_URL: &str = cfg_or!(
    "NODAW_GUMROAD_URL_HARMONIC_SPACE",
    "https://nodawlabs.gumroad.com/l/nodaw-harmonic-space"
);
pub const CHRONOS_DYNAMIC_EQ_URL: &str = cfg_or!(
    "NODAW_GUMROAD_URL_CHRONOS_DYNAMIC_EQ",
    "https://nodawlabs.gumroad.com/l/nodaw-chronos-dynamic-eq"
);
pub const TEMPORAL_PITCH_PORTAL_URL: &str = cfg_or!(
    "NODAW_GUMROAD_URL_TEMPORAL_PITCH_PORTAL",
    "https://nodawlabs.gumroad.com/l/nodaw-temporal-pitch-portal"
);
pub const CHRONOS_WEAVER_URL: &str = cfg_or!(
    "NODAW_GUMROAD_URL_CHRONOS_WEAVER",
    "https://nodawlabs.gumroad.com/l/nodaw-chronos-weaver"
);
