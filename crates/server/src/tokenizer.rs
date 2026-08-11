use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
#[cfg(feature = "local-embeddings")]
use tokenizers::models::bpe::BPE;
#[cfg(feature = "local-embeddings")]
use tokenizers::pre_tokenizers::byte_level::ByteLevel;
#[cfg(feature = "local-embeddings")]
use tokenizers::Tokenizer;

/// bundled file (carried over from old aetheria's assets) is a raw gpt2
/// vocab/merge list, not a ready HF fast-tokenizer json, so we build the
/// BPE model ourselves at startup instead of Tokenizer::from_file
#[derive(Deserialize)]
struct BundledVocab {
    vocab: HashMap<String, u32>,
    merges: Vec<(String, String)>,
}

fn to_ahash_map(vocab: HashMap<String, u32>) -> ahash::AHashMap<String, u32> {
    vocab.into_iter().collect()
}

#[cfg(feature = "local-embeddings")]
static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();

#[cfg(feature = "local-embeddings")]
fn tokenizer() -> &'static Tokenizer {
    TOKENIZER.get_or_init(|| {
        let bundled: BundledVocab = serde_json::from_slice(include_bytes!("../assets/gpt2_tokenizer.json"))
            .expect("bundled gpt2 tokenizer data should always parse");
        let bpe = BPE::builder()
            .vocab_and_merges(to_ahash_map(bundled.vocab), bundled.merges)
            .build()
            .expect("bundled gpt2 vocab/merges should build a valid BPE model");
        let mut tokenizer = Tokenizer::new(bpe);
        tokenizer.with_pre_tokenizer(Some(ByteLevel::new(false, true, true)));
        tokenizer
    })
}

/// real BPE token count instead of the old chars/4 guess when local-embeddings
/// is enabled; falls back to char ratio when disabled or on parse error.
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    #[cfg(feature = "local-embeddings")]
    {
        if let Ok(encoding) = tokenizer().encode(text, false) {
            return encoding.len();
        }
    }
    (text.chars().count() as f64 / 4.0).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_more_tokens_for_longer_text() {
        let short = count_tokens("Hello there.");
        let long = count_tokens("Hello there, this is a considerably longer sentence with more words in it.");
        assert!(long > short);
    }

    #[test]
    fn empty_text_is_zero_tokens() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn matches_expected_ballpark_for_a_known_sentence() {
        // "hello, world!" is 4 gPT-2 BPE tokens: "hello", ",", " world", "!"
        assert_eq!(count_tokens("Hello, world!"), 4);
    }
}
