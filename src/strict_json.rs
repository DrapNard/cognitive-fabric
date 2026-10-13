//! Bounded JSON values with duplicate object names rejected at every nesting level.
use anyhow::{bail, Result};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an unambiguous JSON value")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("Nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("Duplicate JSON object key"));
                    }
                    let Unique(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}
/// Preserve JSON values while rejecting duplicate decoded keys, trailing data,
/// oversized input and the JSON deserializer's ordinary recursion-limit failures.
pub fn value(bytes: &[u8], max_bytes: usize) -> Result<Value> {
    if bytes.len() > max_bytes {
        bail!("JSON input exceeds byte limit")
    }
    let Unique(value) = serde_json::from_slice(bytes)?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_values_and_rejects_decoded_duplicates_without_echoing_keys() {
        let raw = br#"{"a":[null,true,false,-1,18446744073709551615,1.5,"text"],"b":{"a":1}}"#;
        assert_eq!(
            value(raw, 1024).unwrap(),
            serde_json::from_slice::<Value>(raw).unwrap()
        );
        for raw in [
            r#"{"secret-name":1,"secret-name":2}"#,
            r#"{"a":{"x":1,"x":2}}"#,
            r#"[{"x":1,"\u0078":2}]"#,
        ] {
            let error = value(raw.as_bytes(), 1024).unwrap_err().to_string();
            assert!(error.contains("Duplicate JSON object key"));
            assert!(!error.contains("secret-name"));
        }
    }
    #[test]
    fn rejects_trailing_values_excessive_bytes_and_depth() {
        assert!(value(b"{} {}", 1024).is_err());
        assert!(value(b"{}", 1).is_err());
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(value(deep.as_bytes(), 1024).is_err());
        assert!(value(b"1e400", 1024).is_err());
    }
}
