use base64::Engine;
use rand::prelude::*;

use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

const PKCE_VALID_CHARS: &[u8] =
    b"~.-_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const MAX_LEN: usize = 128;

pub fn generate_code_verifier() -> Vec<u8> {
    let mut rng = thread_rng();
    let mut code_verifier = Vec::with_capacity(MAX_LEN);
    for _ in 0..MAX_LEN {
        code_verifier.push(
            *PKCE_VALID_CHARS
                .choose(&mut rng)
                .expect("Error while choosing PKCE valid chars with rand."),
        );
    }

    code_verifier
}

pub fn gen_s256_code_verifier() -> String {
    let code = generate_code_verifier();
    encode_s256(&code)
}

pub fn encode_s256(input: &Vec<u8>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let hash_result = hasher.finalize();
    let base64_hash = BASE64_URL_SAFE_NO_PAD.encode(&hash_result);

    base64_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_gen() {
        let code = generate_code_verifier();
        let encoded = encode_s256(&code);
        assert_eq!(encoded.len(), 43);
    }

    #[test]
    fn test_generate_code_verifier_correct_len() {
        let code = generate_code_verifier();
        assert_eq!(code.len(), 128);
    }

    #[test]
    fn test_can_stringify_code_verifier() {
        let code = generate_code_verifier();
        assert!(String::from_utf8(code).is_ok());
    }
}
