use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::net::{Encoding, Transport};
use crate::{Error, Result};

/// Packs serializable object to bytes based on selected encoding.
pub fn encode<S: Serialize>(obj: S, encoding: Encoding) -> Result<Vec<u8>> {
    let packed: Vec<u8> = match encoding {
        Encoding::Bincode => bincode::serialize(&obj)?,
        Encoding::MsgPack => {
            #[cfg(not(feature = "msgpack_encoding"))]
            panic!(
                "trying to use msgpack encoding, but msgpack_encoding crate feature is not enabled"
            );
            #[cfg(feature = "msgpack_encoding")]
            {
                use rmp_serde::config::StructMapConfig;
                let mut buf = Vec::new();
                obj.serialize(&mut rmp_serde::Serializer::new(&mut buf))?;
                buf
            }
        }
        Encoding::Json => {
            #[cfg(not(feature = "json_encoding"))]
            panic!("trying to use json encoding, but json_encoding crate feature is not enabled");
            #[cfg(feature = "json_encoding")]
            {
                serde_json::to_vec(&obj)?
            }
        }
    };
    Ok(packed)
}

/// Unpacks object from bytes based on selected encoding.
pub fn decode<'de, P: Deserialize<'de>>(bytes: &'de [u8], encoding: Encoding) -> Result<P> {
    let unpacked = match encoding {
        Encoding::Bincode => bincode::deserialize(bytes)?,
        Encoding::MsgPack => {
            #[cfg(not(feature = "msgpack_encoding"))]
            panic!("trying to unpack using msgpack encoding, but msgpack_encoding crate feature is not enabled");
            #[cfg(feature = "msgpack_encoding")]
            {
                use rmp_serde::config::StructMapConfig;
                let mut de = rmp_serde::Deserializer::new(bytes).with_binary();
                Deserialize::deserialize(&mut de)?
            }
        }
        Encoding::Json => {
            #[cfg(not(feature = "json_encoding"))]
            panic!("trying to unpack using json encoding, but json_encoding crate feature is not enabled");
            #[cfg(feature = "json_encoding")]
            {
                serde_json::from_slice(bytes)?
            }
        }
    };
    Ok(unpacked)
}

// TODO allow for different compression modes
/// Compress bytes using lz4.
#[cfg(feature = "lz4")]
pub(crate) fn compress(bytes: &Vec<u8>) -> Result<Vec<u8>> {
    let compressed = lz4::block::compress(bytes.as_slice(), None, true)?;
    Ok(compressed)
}

// /// Takes a payload struct and turns it directly into a serialized message.
// pub(crate) fn msg_bytes_from_payload<P>(payload: P, encoding: &Encoding) -> Result<Vec<u8>>
// where
//     P: Serialize,
//     P: Payload,
// {
//     let msg = Message {
//         type_: payload.r#type(),
//         payload: encode(payload, encoding)?,
//     };
//     Ok(encode(msg, encoding)?)
// }
