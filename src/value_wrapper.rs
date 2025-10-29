

use std::io::{Read, Write};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

/// Wrapper for `serde_json::Value` that implements `BorshSerialize` and `BorshDeserialize`.
///
/// This type bridges the gap between JSON-based state/event representations and the
/// efficient Borsh binary serialization format used for WASM boundary crossings.
/// It allows contract states and events (which are JSON-compatible) to be efficiently
/// transferred between the WASM module and the host runtime.
///
/// The wrapper handles all JSON value types:
/// - Bool: Boolean values
/// - Number: Numeric values (f64, i64, u64)
/// - String: Text values
/// - Array: Ordered collections of values
/// - Object: Key-value maps
/// - Null: Null values
///
/// # Example
///
/// ```ignore
/// use serde_json::json;
///
/// let value = json!({"count": 42, "name": "test"});
/// let wrapper = ValueWrapper(value);
///
/// // Serialize to Borsh binary format
/// let bytes = borsh::to_vec(&wrapper).unwrap();
///
/// // Deserialize back from Borsh
/// let restored: ValueWrapper = borsh::from_slice(&bytes).unwrap();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ValueWrapper(pub Value);

/// Borsh serialization implementation for `ValueWrapper`.
///
/// Serializes JSON values into an efficient binary format using type tags.
/// Each JSON type is prefixed with a discriminator byte to identify its type:
/// - 0: Boolean
/// - 1: Number (with sub-discriminator for f64=0, i64=1, u64=2)
/// - 2: String
/// - 3: Array
/// - 4: Object
/// - 5: Null
impl BorshSerialize for ValueWrapper {
    #[inline]
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match &self.0 {
            // Serialize boolean: type tag (0) + boolean value
            Value::Bool(data) => {
                BorshSerialize::serialize(&0u8, writer)?;
                BorshSerialize::serialize(&data, writer)
            }
            // Serialize number: type tag (1) + numeric sub-type tag + value
            Value::Number(data) => {
                BorshSerialize::serialize(&1u8, writer)?;
                'data: {
                    // Try f64 first
                    if data.is_f64() {
                        let Some(data) = data.as_f64() else {
                            break 'data;
                        };
                        BorshSerialize::serialize(&0u8, writer)?;
                        return BorshSerialize::serialize(&data, writer);
                    }
                    // Try i64
                    else if data.is_i64() {
                        let Some(data) = data.as_i64() else {
                            break 'data;
                        };
                        BorshSerialize::serialize(&1u8, writer)?;
                        return BorshSerialize::serialize(&data, writer);
                    }
                    // Try u64
                    else if data.is_u64() {
                        let Some(data) = data.as_u64() else {
                            break 'data;
                        };
                        BorshSerialize::serialize(&2u8, writer)?;
                        return BorshSerialize::serialize(&data, writer);
                    }
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid number type",
                ))
            }
            // Serialize string: type tag (2) + string data
            Value::String(data) => {
                BorshSerialize::serialize(&2u8, writer)?;
                BorshSerialize::serialize(&data, writer)
            }
            // Serialize array: type tag (3) + length + elements
            Value::Array(data) => {
                BorshSerialize::serialize(&3u8, writer)?;
                BorshSerialize::serialize(&(data.len() as u32), writer)?;
                for element in data {
                    let element = ValueWrapper(element.to_owned());
                    BorshSerialize::serialize(&element, writer)?;
                }
                Ok(())
            }
            // Serialize object: type tag (4) + length + key-value pairs
            Value::Object(data) => {
                BorshSerialize::serialize(&4u8, writer)?;
                BorshSerialize::serialize(&(data.len() as u32), writer)?;
                for (key, value) in data {
                    BorshSerialize::serialize(&key, writer)?;
                    let value = ValueWrapper(value.to_owned());
                    BorshSerialize::serialize(&value, writer)?;
                }
                Ok(())
            }
            // Serialize null: just type tag (5)
            Value::Null => BorshSerialize::serialize(&5u8, writer),
        }
    }
}

/// Borsh deserialization implementation for `ValueWrapper`.
///
/// Deserializes binary Borsh data back into JSON values by reading type discriminators
/// and reconstructing the appropriate JSON value type. This is the inverse of the
/// serialization process.
///
/// The type tags used are:
/// - 0: Boolean
/// - 1: Number (with sub-discriminator for f64=0, i64=1, u64=2)
/// - 2: String
/// - 3: Array
/// - 4: Object
/// - 5: Null
impl BorshDeserialize for ValueWrapper {
    #[inline]
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        // Read the type discriminator byte
        let order: u8 = BorshDeserialize::deserialize_reader(reader)?;
        match order {
            // Type 0: Boolean
            0 => {
                let data: bool = BorshDeserialize::deserialize_reader(reader)?;
                Ok(ValueWrapper(Value::Bool(data)))
            }
            // Type 1: Number (requires reading numeric sub-type)
            1 => {
                let internal_order: u8 =
                    BorshDeserialize::deserialize_reader(reader)?;
                match internal_order {
                    // Sub-type 0: f64
                    0 => {
                        let data: f64 =
                            BorshDeserialize::deserialize_reader(reader)?;
                        let Some(data_f64) = Number::from_f64(data) else {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidInput,
                                format!("Invalid f64 Number: {}", data),
                            ));
                        };
                        Ok(ValueWrapper(Value::Number(data_f64)))
                    }
                    // Sub-type 1: i64
                    1 => {
                        let data: i64 =
                            BorshDeserialize::deserialize_reader(reader)?;
                        Ok(ValueWrapper(Value::Number(Number::from(data))))
                    }
                    // Sub-type 2: u64
                    2 => {
                        let data: u64 =
                            BorshDeserialize::deserialize_reader(reader)?;
                        Ok(ValueWrapper(Value::Number(Number::from(data))))
                    }
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "Invalid Number representation: {}",
                            internal_order
                        ),
                    )),
                }
            }
            // Type 2: String
            2 => {
                let data: String =
                    BorshDeserialize::deserialize_reader(reader)?;
                Ok(ValueWrapper(Value::String(data)))
            }
            // Type 3: Array (read length, then elements)
            3 => {
                let len = u32::deserialize_reader(reader)?;
                if len == 0 {
                    Ok(ValueWrapper(Value::Array(Vec::new())))
                } else {
                    let mut result = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        result
                            .push(ValueWrapper::deserialize_reader(reader)?.0);
                    }
                    Ok(ValueWrapper(Value::Array(result)))
                }
            }
            // Type 4: Object (read length, then key-value pairs)
            4 => {
                let len = u32::deserialize_reader(reader)?;
                let mut result = Map::new();
                for _ in 0..len {
                    let key = String::deserialize_reader(reader)?;
                    let value = ValueWrapper::deserialize_reader(reader)?;
                    result.insert(key, value.0);
                }
                Ok(ValueWrapper(Value::Object(result)))
            }
            // Type 5: Null
            5 => Ok(ValueWrapper(Value::Null)),
            // Unknown type discriminator
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid Value representation: {}", order),
            )),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_wrapper() {
        let value = ValueWrapper(Value::String("test".to_owned()));
        let vec = borsh::to_vec(&value).unwrap();
        let value2: ValueWrapper = BorshDeserialize::try_from_slice(&vec).unwrap();
        assert_eq!(value, value2);
    }
}
