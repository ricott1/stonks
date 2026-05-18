//! Stonks-specific SSH-auth constants and helpers. The SSH server runtime
//! itself comes from `frittura_ssh_core`; this module just holds the bits that
//! identify a user's save (salts, username length rules, the hashed
//! `Password` type).

use rand::distr::Alphanumeric;
use rand::RngExt;

pub type Password = [u8; 32];

pub static AUTH_PASSWORD_SALT: &str = "gbasfhgE4Fvb";
pub static AUTH_PUBLIC_KEY_SALT: &str = "fa2RR4fq9XX9";

pub const MIN_USERNAME_LENGTH: usize = 3;
pub const MAX_USERNAME_LENGTH: usize = 12;

pub fn generate_user_id() -> String {
    let buf_id = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .collect::<Vec<u8>>()
        .to_ascii_lowercase();
    std::str::from_utf8(buf_id.as_slice())
        .expect("Failed to generate user id string")
        .to_string()
}
