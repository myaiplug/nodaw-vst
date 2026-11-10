//! Trial and license state of NoDAW Screw: a 14-day full trial from the first launch, then
//! activation with a Gumroad license key. After the trial, unlicensed copies keep working (audio,
//! presets and sessions) but drop to silence for half a second every 45 seconds (see
//! [`Dropout`]).
//!
//! The implementation is shared by every NoDAW plugin (`nodaw_kit::license`); this module is
//! Screw's product (its Gumroad product, its HMAC key and its state folder, all unchanged from
//! Screw 0.2.0, so existing trials and activations carry over) and the names the rest of the
//! plugin uses.

use std::sync::Arc;

pub use nodaw_kit::license::{
    evaluate, now_secs, parse_verify, Dropout, Manager, ParseError, Product, Record, Status, Store,
    Verdict, DAY, RECHECK_SECS, TRIAL_DAYS,
};

/// TODO(owner): the Gumroad product ID of NoDAW Screw (Gumroad dashboard: Product > Content >
/// License key > "product_id"). Until it is set, activation tells the user it is not available.
pub const GUMROAD_PRODUCT_ID: &str = "TODO_GUMROAD_PRODUCT_ID";

/// TODO(owner): the public Gumroad page the Buy button opens.
pub const BUY_URL: &str = "https://nodawlabs.gumroad.com/l/nodaw-screw";

/// NoDAW Screw as a product.
pub static PRODUCT: Product = Product {
    name: "NoDAW Screw",
    folder: "Screw",
    gumroad_product_id: GUMROAD_PRODUCT_ID,
    buy_url: BUY_URL,
    hmac_key: b"nodaw-screw/license/v1/7c1e5b0a-93f4-4d1e-b6a2-2f8d0c4e9a17",
    user_agent: concat!("NoDAW-Screw/", env!("CARGO_PKG_VERSION")),
};

/// The manager over Screw's state files.
pub fn load_default() -> Arc<Manager> {
    Manager::load_default(&PRODUCT)
}

/// Opens Screw's Gumroad page in the default browser.
pub fn open_buy_page() -> bool {
    nodaw_kit::license::open_buy_page(BUY_URL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_stays_where_screw_0_2_kept_it() {
        let s = Store::default_paths(&PRODUCT);
        if let Some(m) = &s.main {
            assert!(
                m.ends_with("NoDAW Labs/Screw/license.json"),
                "{}",
                m.display()
            );
        }
        if let Some(sh) = &s.shadow {
            assert!(sh.ends_with("NoDAW Labs/Screw/.cache"), "{}", sh.display());
        }
        assert_eq!(
            s.key,
            b"nodaw-screw/license/v1/7c1e5b0a-93f4-4d1e-b6a2-2f8d0c4e9a17"
        );
    }

    #[test]
    fn a_record_signed_with_screws_key_round_trips_and_another_key_is_tampering() {
        let r = Record::fresh(1_700_000_000);
        let text = r.to_json(PRODUCT.hmac_key);
        assert!(Record::from_json(&text, PRODUCT.hmac_key).is_ok());
        assert!(matches!(
            Record::from_json(&text, b"another key"),
            Err(ParseError::Tampered)
        ));
    }

    #[test]
    fn activation_waits_for_the_product_id() {
        assert!(!PRODUCT.activation_ready());
    }
}
