use rand::Rng;

const CODE_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const CODE_LEN: usize = 6;

pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CODE_CHARSET.len());
            CODE_CHARSET[idx] as char
        })
        .collect()
}

pub fn normalize_code(code: &str) -> Option<String> {
    let normalized = code.trim().to_uppercase();
    if normalized.len() != CODE_LEN {
        return None;
    }

    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        return None;
    }

    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_code_is_six_uppercase_alphanumeric() {
        let code = generate_code();
        assert_eq!(code.len(), CODE_LEN);
        assert!(
            code.chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        );
    }

    #[test]
    fn normalize_code_accepts_case_insensitive_input() {
        assert_eq!(normalize_code("a3k9z1").as_deref(), Some("A3K9Z1"));
        assert_eq!(normalize_code(" A3K9Z1 ").as_deref(), Some("A3K9Z1"));
        assert!(normalize_code("abc").is_none());
        assert!(normalize_code("A3K9Z!").is_none());
    }
}
