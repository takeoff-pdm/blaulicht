//! JSON for server messages that keeps every map representable.
//!
//! `serde_json` refuses maps whose keys are tuples or structs, and the show
//! state has several (`overrides` by `(universe, channel)`, fixture states
//! by `(group, fixture)`, animations by selection). [`to_value`] is a serde
//! serializer that builds the same `serde_json::Value` as
//! `serde_json::to_value`, except that a map key which isn't a string,
//! number or bool becomes its JSON text (`(0, 5)` becomes `"[0,5]"`). Floats
//! that are not finite become `null`, as in `serde_json`.

use serde::{
    ser::{self, Error as _},
    Serialize,
};
use serde_json::{Error, Map, Number, Value as Json};

/// `value` as JSON text (see the module docs).
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(&to_value(value)?)?)
}

/// `value` as a JSON value (see the module docs).
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Json, Error> {
    value.serialize(ValueSerializer)
}

/// The text a map key becomes.
fn key_text(key: Json) -> String {
    match key {
        Json::String(text) => text,
        Json::Number(number) => number.to_string(),
        Json::Bool(flag) => flag.to_string(),
        other => other.to_string(),
    }
}

fn float(value: f64) -> Json {
    Number::from_f64(value).map_or(Json::Null, Json::Number)
}

struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Json;
    type Error = Error;
    type SerializeSeq = SeqBuilder;
    type SerializeTuple = SeqBuilder;
    type SerializeTupleStruct = SeqBuilder;
    type SerializeTupleVariant = VariantBuilder<SeqBuilder>;
    type SerializeMap = MapBuilder;
    type SerializeStruct = MapBuilder;
    type SerializeStructVariant = VariantBuilder<MapBuilder>;

    fn serialize_bool(self, v: bool) -> Result<Json, Error> {
        Ok(Json::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_i128(self, v: i128) -> Result<Json, Error> {
        i64::try_from(v)
            .map(Json::from)
            .or_else(|_| u64::try_from(v).map(Json::from))
            .map_err(|_| Error::custom("integer out of range"))
    }
    fn serialize_u8(self, v: u8) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<Json, Error> {
        Ok(v.into())
    }
    fn serialize_u128(self, v: u128) -> Result<Json, Error> {
        u64::try_from(v)
            .map(Json::from)
            .map_err(|_| Error::custom("integer out of range"))
    }
    fn serialize_f32(self, v: f32) -> Result<Json, Error> {
        Ok(float(v.into()))
    }
    fn serialize_f64(self, v: f64) -> Result<Json, Error> {
        Ok(float(v))
    }
    fn serialize_char(self, v: char) -> Result<Json, Error> {
        Ok(Json::String(v.to_string()))
    }
    fn serialize_str(self, v: &str) -> Result<Json, Error> {
        Ok(Json::String(v.to_string()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Json, Error> {
        Ok(Json::Array(
            v.iter().map(|byte| Json::from(*byte)).collect(),
        ))
    }
    fn serialize_none(self) -> Result<Json, Error> {
        Ok(Json::Null)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Json, Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<Json, Error> {
        Ok(Json::Null)
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Json, Error> {
        Ok(Json::Null)
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Json, Error> {
        Ok(Json::String(variant.to_string()))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Json, Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Json, Error> {
        let mut map = Map::new();
        map.insert(variant.to_string(), value.serialize(self)?);
        Ok(Json::Object(map))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<SeqBuilder, Error> {
        Ok(SeqBuilder(Vec::with_capacity(len.unwrap_or(0))))
    }
    fn serialize_tuple(self, len: usize) -> Result<SeqBuilder, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SeqBuilder, Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantBuilder<SeqBuilder>, Error> {
        Ok(VariantBuilder {
            variant,
            inner: self.serialize_seq(Some(len))?,
        })
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<MapBuilder, Error> {
        Ok(MapBuilder {
            map: Map::new(),
            key: None,
        })
    }
    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<MapBuilder, Error> {
        self.serialize_map(Some(len))
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantBuilder<MapBuilder>, Error> {
        Ok(VariantBuilder {
            variant,
            inner: self.serialize_map(Some(len))?,
        })
    }
}

struct SeqBuilder(Vec<Json>);

impl SeqBuilder {
    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.0.push(value.serialize(ValueSerializer)?);
        Ok(())
    }
}

impl ser::SerializeSeq for SeqBuilder {
    type Ok = Json;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Json::Array(self.0))
    }
}

impl ser::SerializeTuple for SeqBuilder {
    type Ok = Json;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Json::Array(self.0))
    }
}

impl ser::SerializeTupleStruct for SeqBuilder {
    type Ok = Json;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.push(value)
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Json::Array(self.0))
    }
}

struct MapBuilder {
    map: Map<String, Json>,
    key: Option<String>,
}

impl ser::SerializeMap for MapBuilder {
    type Ok = Json;
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.key = Some(key_text(key.serialize(ValueSerializer)?));
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = self
            .key
            .take()
            .ok_or_else(|| Error::custom("map value without a key"))?;
        self.map.insert(key, value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Json::Object(self.map))
    }
}

impl ser::SerializeStruct for MapBuilder {
    type Ok = Json;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.map
            .insert(key.to_string(), value.serialize(ValueSerializer)?);
        Ok(())
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Json::Object(self.map))
    }
}

/// A tuple or struct variant: `{"Variant": [..]}` or `{"Variant": {..}}`.
struct VariantBuilder<T> {
    variant: &'static str,
    inner: T,
}

impl<T> VariantBuilder<T> {
    fn wrap(variant: &'static str, value: Json) -> Json {
        let mut map = Map::new();
        map.insert(variant.to_string(), value);
        Json::Object(map)
    }
}

impl ser::SerializeTupleVariant for VariantBuilder<SeqBuilder> {
    type Ok = Json;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.inner.push(value)
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Self::wrap(self.variant, Json::Array(self.inner.0)))
    }
}

impl ser::SerializeStructVariant for VariantBuilder<MapBuilder> {
    type Ok = Json;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        ser::SerializeStruct::serialize_field(&mut self.inner, key, value)
    }
    fn end(self) -> Result<Json, Error> {
        Ok(Self::wrap(self.variant, Json::Object(self.inner.map)))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        net::SocketAddr,
    };

    use serde::Serialize;
    use serde_json::json;

    #[derive(Serialize)]
    enum Shape {
        Unit,
        Newtype(u8),
        Tuple(u8, bool),
        Struct { a: Option<u8>, b: f64 },
    }

    #[test]
    fn composite_keys_become_their_json_text_and_the_rest_matches_serde_json() {
        let mut overrides = BTreeMap::new();
        overrides.insert((0usize, 5usize), 255u8);
        let mut names = HashMap::new();
        names.insert(3u8, "three");
        let value = (
            overrides,
            names,
            vec![Shape::Unit, Shape::Newtype(7), Shape::Tuple(1, true)],
        );
        assert_eq!(
            super::to_value(&value).unwrap(),
            json!([
                {"[0,5]": 255},
                {"3": "three"},
                ["Unit", {"Newtype": 7}, {"Tuple": [1, true]}]
            ])
        );
        // Types serde_json handles come out the same, including the ones
        // that serialize differently for human-readable formats.
        let plain = (
            Shape::Struct { a: None, b: 0.5 },
            "10.0.0.5:6454".parse::<SocketAddr>().unwrap(),
            std::time::Duration::from_millis(1500),
        );
        assert_eq!(
            super::to_value(&plain).unwrap(),
            serde_json::to_value(&plain).unwrap()
        );
        assert_eq!(super::to_value(&f32::NAN).unwrap(), json!(null));
    }
}
