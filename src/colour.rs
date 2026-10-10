//! Hex colour parsing for the per-window accent colour. Accepts `#rgb` and `#rrggbb`
//! (case-insensitive), the forms both apps' chrome banners render.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Error, PartialEq)]
pub enum ColourError {
    #[error("colour must start with '#': {0:?}")]
    NoHash(String),
    #[error("colour must be #rgb or #rrggbb: {0:?}")]
    BadLength(String),
    #[error("colour has non-hex digits: {0:?}")]
    BadDigit(String),
}

impl Colour {
    pub fn parse(s: &str) -> Result<Colour, ColourError> {
        let rest = s
            .strip_prefix('#')
            .ok_or_else(|| ColourError::NoHash(s.to_string()))?;
        if !rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ColourError::BadDigit(s.to_string()));
        }
        let d: Vec<u8> = rest.bytes().map(hex_val).collect();
        match d[..] {
            [r, g, b] => Ok(Colour {
                r: r * 17,
                g: g * 17,
                b: b * 17,
            }),
            [r1, r0, g1, g0, b1, b0] => Ok(Colour {
                r: r1 * 16 + r0,
                g: g1 * 16 + g0,
                b: b1 * 16 + b0,
            }),
            _ => Err(ColourError::BadLength(s.to_string())),
        }
    }

    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// The value of one hex digit; the caller has already checked `b.is_ascii_hexdigit()`.
fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_digit() {
        assert_eq!(
            Colour::parse("#0F8A8A").unwrap(),
            Colour {
                r: 15,
                g: 138,
                b: 138
            }
        );
    }

    #[test]
    fn parses_three_digit_shorthand() {
        assert_eq!(
            Colour::parse("#0a0").unwrap(),
            Colour { r: 0, g: 170, b: 0 }
        );
    }

    #[test]
    fn round_trips_to_lowercase_hex() {
        assert_eq!(Colour::parse("#0F8A8A").unwrap().hex(), "#0f8a8a");
    }

    #[test]
    fn rejects_missing_hash() {
        assert_eq!(
            Colour::parse("0f8a8a"),
            Err(ColourError::NoHash("0f8a8a".into()))
        );
    }

    #[test]
    fn rejects_bad_length() {
        assert!(matches!(
            Colour::parse("#ff"),
            Err(ColourError::BadLength(_))
        ));
    }

    #[test]
    fn rejects_non_hex() {
        assert!(matches!(
            Colour::parse("#gggggg"),
            Err(ColourError::BadDigit(_))
        ));
    }

    #[test]
    fn rejects_non_ascii_without_panic() {
        assert!(matches!(
            Colour::parse("#中中"),
            Err(ColourError::BadDigit(_))
        ));
    }

    #[test]
    fn parses_three_digit_uppercase() {
        assert_eq!(
            Colour::parse("#0A0").unwrap(),
            Colour { r: 0, g: 170, b: 0 }
        );
    }

    #[test]
    fn rejects_plus_prefixed_hex() {
        assert!(matches!(
            Colour::parse("#+a0000"),
            Err(ColourError::BadDigit(_))
        ));
        assert!(matches!(
            Colour::parse("#0000 0"),
            Err(ColourError::BadDigit(_))
        ));
    }
}
