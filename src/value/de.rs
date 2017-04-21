use std::fmt;
use std::vec;

use serde::de::{
    Deserialize,
    DeserializeSeed,
    Deserializer,
    EnumAccess,
    Error as SError,
    MapAccess,
    SeqAccess,
    Unexpected,
    VariantAccess,
    Visitor,
};
use num_traits::NumCast;

use super::Value;
use mapping::Mapping;
use error::Error;

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("any YAML value")
            }

            fn visit_bool<E>(self, b: bool) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::Bool(b))
            }

            fn visit_i64<E>(self, i: i64) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::I64(i))
            }

            fn visit_u64<E>(self, u: u64) -> Result<Value, E>
                where E: SError,
            {
                match NumCast::from(u) {
                    Some(i) => Ok(Value::I64(i)),
                    None => Ok(Value::String(u.to_string())),
                }
            }

            fn visit_f64<E>(self, f: f64) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::F64(f))
            }

            fn visit_str<E>(self, s: &str) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::String(s.to_owned()))
            }

            fn visit_string<E>(self, s: String) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::String(s))
            }

            fn visit_unit<E>(self) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::Null)
            }

            fn visit_none<E>(self) -> Result<Value, E>
                where E: SError,
            {
                Ok(Value::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
                where D: Deserializer<'de>
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_seq<V>(self, mut visitor: V) -> Result<Value, V::Error>
                where V: SeqAccess<'de>
            {
                let mut vec = Vec::new();

                while let Some(element) = visitor.next_element()? {
                    vec.push(element);
                }

                Ok(Value::Sequence(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Value, V::Error>
                where V: MapAccess<'de>
            {
                let mut values = Mapping::new();

                while let Some((key, value)) = visitor.next_entry()? {
                    values.insert(key, value);
                }

                Ok(Value::Mapping(values))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

impl<'de> Deserializer<'de> for Value {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(v) => visitor.visit_bool(v),
            Value::I64(i) => visitor.visit_i64(i),
            Value::F64(f) => visitor.visit_f64(f),
            Value::String(v) => visitor.visit_string(v),
            Value::Sequence(v) => {
                let len = v.len();
                let mut deserializer = SeqDeserializer::new(v);
                let seq = visitor.visit_seq(&mut deserializer)?;
                let remaining = deserializer.iter.len();
                if remaining == 0 {
                    Ok(seq)
                } else {
                    Err(Error::invalid_length(len, &"fewer elements in sequence"))
                }
            }
            Value::Mapping(v) => {
                let len = v.len();
                let mut deserializer = MapDeserializer::new(v);
                let map = visitor.visit_map(&mut deserializer)?;
                let remaining = deserializer.iter.len();
                if remaining == 0 {
                    Ok(map)
                } else {
                    Err(Error::invalid_length(len, &"fewer elements in map"))
                }
            }
        }
    }

    #[inline]
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        match self {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    #[inline]
    fn deserialize_enum<V>(self,
                           _name: &str,
                           _variants: &'static [&'static str],
                           visitor: V)
                           -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        let (variant, value) = match self {
            Value::Mapping(value) => {
                let mut iter = value.into_iter();
                let (variant, value) = match iter.next() {
                    Some(v) => v,
                    None => {
                        return Err(Error::invalid_value(Unexpected::Map,
                                                                   &"map with a single key"));
                    }
                };
                // enums are encoded in json as maps with a single key:value pair
                if iter.next().is_some() {
                    return Err(Error::invalid_value(Unexpected::Map,
                                                               &"map with a single key"));
                }
                (variant, Some(value))
            }
            Value::String(variant) => (Value::String(variant), None),
            other => {
                return Err(Error::invalid_type(other.unexpected(), &"string or map"));
            }
        };

        visitor.visit_enum(EnumDeserializer {
                               variant: variant,
                               value: value,
                           })
    }

    #[inline]
    fn deserialize_newtype_struct<V>(self,
                                     _name: &'static str,
                                     visitor: V)
                                     -> Result<V::Value, Self::Error>
        where V: Visitor<'de>
    {
        visitor.visit_newtype_struct(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
        ignored_any
    }
}

struct EnumDeserializer {
    variant: Value,
    value: Option<Value>,
}

impl<'de> EnumAccess<'de> for EnumDeserializer {
    type Error = Error;
    type Variant = VariantDeserializer;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantDeserializer), Error>
        where V: DeserializeSeed<'de>
    {
        let visitor = VariantDeserializer { value: self.value };
        seed.deserialize(self.variant).map(|v| (v, visitor))
    }
}

struct VariantDeserializer {
    value: Option<Value>,
}

impl<'de> VariantAccess<'de> for VariantDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        match self.value {
            Some(value) => Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Error>
        where T: DeserializeSeed<'de>
    {
        match self.value {
            Some(value) => seed.deserialize(value),
            None => {
                Err(Error::invalid_type(Unexpected::UnitVariant, &"newtype variant"))
            }
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        match self.value {
            Some(Value::Sequence(v)) => {
                Deserializer::deserialize_any(SeqDeserializer::new(v), visitor)
            }
            Some(other) => {
                Err(Error::invalid_type(other.unexpected(), &"tuple variant"))
            }
            None => Err(Error::invalid_type(Unexpected::UnitVariant, &"tuple variant")),
        }
    }

    fn struct_variant<V>(self,
                       _fields: &'static [&'static str],
                       visitor: V)
                       -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        match self.value {
            Some(Value::Mapping(v)) => {
                Deserializer::deserialize_any(MapDeserializer::new(v), visitor)
            }
            Some(other) => {
                Err(Error::invalid_type(other.unexpected(), &"struct variant"))
            }
            _ => Err(Error::invalid_type(Unexpected::UnitVariant, &"struct variant")),
        }
    }
}

struct SeqDeserializer {
    iter: vec::IntoIter<Value>,
}

impl SeqDeserializer {
    fn new(vec: Vec<Value>) -> Self {
        SeqDeserializer { iter: vec.into_iter() }
    }
}

impl<'de> Deserializer<'de> for SeqDeserializer {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(mut self, visitor: V) -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        let len = self.iter.len();
        if len == 0 {
            visitor.visit_unit()
        } else {
            let ret = visitor.visit_seq(&mut self)?;
            let remaining = self.iter.len();
            if remaining == 0 {
                Ok(ret)
            } else {
                Err(Error::invalid_length(len, &"fewer elements in sequence"))
            }
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }
}

impl<'de> SeqAccess<'de> for SeqDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
        where T: DeserializeSeed<'de>
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

struct MapDeserializer {
    iter: <Mapping as IntoIterator>::IntoIter,
    value: Option<Value>,
}

impl MapDeserializer {
    fn new(map: Mapping) -> Self {
        MapDeserializer {
            iter: map.into_iter(),
            value: None,
        }
    }
}

impl<'de> MapAccess<'de> for MapDeserializer {
    type Error = Error;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
        where T: DeserializeSeed<'de>
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(key).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Error>
        where T: DeserializeSeed<'de>
    {
        match self.value.take() {
            Some(value) => seed.deserialize(value),
            None => panic!("visit_value called before visit_key"),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

impl<'de> Deserializer<'de> for MapDeserializer {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Error>
        where V: Visitor<'de>
    {
        visitor.visit_map(self)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }
}

impl Value {
    fn unexpected(&self) -> Unexpected {
        match *self {
            Value::Null => Unexpected::Unit,
            Value::Bool(b) => Unexpected::Bool(b),
            Value::I64(i) => Unexpected::Signed(i),
            Value::F64(f) => Unexpected::Float(f),
            Value::String(ref s) => Unexpected::Str(s),
            Value::Sequence(_) => Unexpected::Seq,
            Value::Mapping(_) => Unexpected::Map,
        }
    }
}