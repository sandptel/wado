//! `cargo run -p wado-mobile-core --features bindgen --bin uniffi-bindgen -- generate --library
//! <libwado_mobile_core.so> --language kotlin --out-dir …` (scripts/android-core.sh).
fn main() {
    uniffi::uniffi_bindgen_main()
}
