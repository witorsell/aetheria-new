use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::nomic_bert::{l2_normalize, mean_pooling, Config, NomicBertModel};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

/// nomic-embed-text-v1.5, same model old aetheria's srv/embedding/local.ts
/// used server-side. retrieval-tuned (not a generic MiniLM-type model).
/// the model itself supports up to 8192 tokens, but MAX_TOKENS below caps
/// input at 2048 - chat messages routinely blow past a smaller window like
/// 512, so this still comfortably covers normal messages without paying
/// full 8192-token CPU inference cost on every embed call.
/// downloaded once from HF, cached in the usual hf-hub dir, free after that
const MODEL_REPO: &str = "nomic-ai/nomic-embed-text-v1.5";
const MAX_TOKENS: usize = 2048;

/// nomic-embed-text-v1.5 embeds queries and stored documents asymmetrically:
/// each input needs a task-specific prefix telling the model which one it
/// is. matches old aetheria's own `PREFIX` table.
pub enum EmbedMode {
    Query,
    Document,
}

impl EmbedMode {
    fn prefix(&self) -> &'static str {
        match self {
            EmbedMode::Query => "search_query: ",
            EmbedMode::Document => "search_document: ",
        }
    }
}

struct LocalEmbedder {
    model: NomicBertModel,
    tokenizer: Tokenizer,
    device: Device,
}

async fn load_embedder() -> Result<LocalEmbedder, String> {
    let repo = MODEL_REPO.to_string();
    tokio::task::spawn_blocking(move || {
        let api = hf_hub::api::sync::Api::new().map_err(|e| e.to_string())?;
        let repo_api = api.model(repo);
        let config_path = repo_api.get("config.json").map_err(|e| e.to_string())?;
        let tokenizer_path = repo_api.get("tokenizer.json").map_err(|e| e.to_string())?;
        let weights_path = repo_api.get("model.safetensors").map_err(|e| e.to_string())?;

        let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

        let mut tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| e.to_string())?;
        tokenizer
            .with_padding(Some(PaddingParams::default()))
            .with_truncation(Some(TruncationParams { max_length: MAX_TOKENS, ..Default::default() }))
            .map_err(|e| e.to_string())?;

        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device).map_err(|e| e.to_string())?
        };
        let model = NomicBertModel::load(vb, &config).map_err(|e| e.to_string())?;

        Ok(LocalEmbedder { model, tokenizer, device })
    })
    .await
    .map_err(|e| e.to_string())?
}

static EMBEDDER: tokio::sync::OnceCell<std::sync::Arc<tokio::sync::Mutex<LocalEmbedder>>> = tokio::sync::OnceCell::const_new();

async fn embedder() -> Result<std::sync::Arc<tokio::sync::Mutex<LocalEmbedder>>, String> {
    EMBEDDER
        .get_or_try_init(|| async { load_embedder().await.map(|e| std::sync::Arc::new(tokio::sync::Mutex::new(e))) })
        .await
        .cloned()
}

/// embeds a batch of texts locally, no external API call. returns one
/// 768-dimension vector per input, in the same order.
pub async fn embed(inputs: &[String], mode: EmbedMode) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let embedder = embedder().await?;
    let prefix = mode.prefix();
    let inputs: Vec<String> = inputs.iter().map(|text| format!("{prefix}{text}")).collect();
    tokio::task::spawn_blocking(move || {
        let guard = embedder.blocking_lock();
        let LocalEmbedder { model, tokenizer, device } = &*guard;

        let encodings = tokenizer.encode_batch(inputs, true).map_err(|e| e.to_string())?;
        let token_ids = encodings
            .iter()
            .map(|e| Tensor::new(e.get_ids(), device))
            .collect::<candle_core::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let attention_mask = encodings
            .iter()
            .map(|e| Tensor::new(e.get_attention_mask(), device))
            .collect::<candle_core::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;

        let token_ids = Tensor::stack(&token_ids, 0).map_err(|e| e.to_string())?;
        let attention_mask = Tensor::stack(&attention_mask, 0).map_err(|e| e.to_string())?;

        let output = model.forward(&token_ids, None, Some(&attention_mask)).map_err(|e| e.to_string())?;
        let pooled = mean_pooling(&output, &attention_mask).map_err(|e| e.to_string())?;
        let pooled = l2_normalize(&pooled).map_err(|e| e.to_string())?;

        let (n_sentences, _) = pooled.dims2().map_err(|e| e.to_string())?;
        let mut result = Vec::with_capacity(n_sentences);
        for i in 0..n_sentences {
            let row = pooled.get(i).map_err(|e| e.to_string())?;
            result.push(row.to_vec1::<f32>().map_err(|e| e.to_string())?);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
