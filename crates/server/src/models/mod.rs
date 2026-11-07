pub mod account;
pub mod chat;
pub mod character;
pub mod lorebook;
pub mod memory_chunk;
pub mod message;
pub mod persona;
pub mod preset;
pub mod regex_script;
pub mod settings;
pub mod theme;
pub mod user;
pub mod tavern;
pub mod group;

/// writes a map's entries sorted by key. a HashMap hands its entries out in a
/// different order every time one is built, so the same data came out as
/// different bytes on every request. the Cloudflare worker in front of the site
/// fetches a long response as separate byte-range requests and glues the pieces
/// together, which only works when every request returns identical bytes. with
/// shuffled orders the glued result lost or repeated messages: a chat loaded
/// with some of its messages missing, or not at all
pub fn serialize_sorted<S, V>(map: &std::collections::HashMap<String, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    V: serde::Serialize,
{
    use serde::Serialize;
    map.iter().collect::<std::collections::BTreeMap<_, _>>().serialize(serializer)
}
