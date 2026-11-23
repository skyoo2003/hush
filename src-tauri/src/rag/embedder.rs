//! Text -> L2-normalized vectors. ADR-013: bge-small-en-v1.5 on candle CPU (Accelerate on macOS).
//! Calls block; run them in `spawn_blocking` or a worker.

use std::collections::HashMap;
use std::path::Path;

use candle_core::{DType, Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use sha2::{Digest, Sha256};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use super::error::RagError;
use crate::ingest::chunker::TokenCounter;
use crate::security::paths::ScopedPath;

pub const BATCH: usize = 32;
const MAX_TOKENS: usize = 512;

pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn model_id(&self) -> &str;
    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RagError>;
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, RagError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooling {
    Cls,
    Mean,
}

#[derive(Debug)]
pub struct ModelSpec {
    pub id: &'static str,
    pub dim: usize,
    pub pooling: Pooling,
    pub query_prefix: &'static str,
    pub passage_prefix: &'static str,
}

/// BERT-architecture models this build can load. ADR-013 picks the first.
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "bge-small-en-v1.5",
        dim: 384,
        pooling: Pooling::Cls,
        query_prefix: "Represent this sentence for searching relevant passages: ",
        passage_prefix: "",
    },
    ModelSpec {
        id: "multilingual-e5-small",
        dim: 384,
        pooling: Pooling::Mean,
        query_prefix: "query: ",
        passage_prefix: "passage: ",
    },
];

/// `models/{id}/manifest.json`: lowercase hex sha256 per file name.
#[derive(serde::Deserialize)]
struct Manifest {
    files: HashMap<String, String>,
}

pub struct BertEmbedder {
    spec: &'static ModelSpec,
    model: BertModel,
    tokenizer: Tokenizer,
}

impl BertEmbedder {
    /// Reads `models_dir/{id}/`, checks every file against `manifest.json`, then builds the
    /// model from the bytes that were hashed, so a file swapped after the check is never loaded.
    pub fn load(models_dir: &Path, id: &str) -> Result<Self, RagError> {
        let spec = MODELS
            .iter()
            .find(|s| s.id == id)
            .ok_or(RagError::UnknownModel)?;
        let read = |name: &str| {
            let p = ScopedPath::new(models_dir, &Path::new(spec.id).join(name))?;
            std::fs::read(p.as_path()).map_err(RagError::Io)
        };
        let manifest: Manifest =
            serde_json::from_slice(&read("manifest.json")?).map_err(RagError::Manifest)?;
        let verified = |name: &'static str| {
            let data = read(name)?;
            let want = manifest.files.get(name).map(|h| h.to_ascii_lowercase());
            if want.as_deref() == Some(hex(&Sha256::digest(&data)).as_str()) {
                Ok(data)
            } else {
                Err(RagError::HashMismatch(name))
            }
        };
        let config = verified("config.json")?;
        let tokenizer = verified("tokenizer.json")?;
        let weights = verified("model.safetensors")?;

        let config: Config = serde_json::from_slice(&config).map_err(RagError::Manifest)?;
        let mut tokenizer = Tokenizer::from_bytes(tokenizer).map_err(|_| RagError::Tokenizer)?;
        let vb = VarBuilder::from_buffered_safetensors(weights, DType::F32, &Device::Cpu)?;
        let model = BertModel::load(vb, &config)?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|_| RagError::Tokenizer)?;
        Ok(Self {
            spec,
            model,
            tokenizer,
        })
    }

    fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, RagError> {
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(BATCH) {
            let enc = self
                .tokenizer
                .encode_batch(batch.to_vec(), true)
                .map_err(|_| RagError::Tokenizer)?;
            let ids: Vec<Vec<u32>> = enc.iter().map(|e| e.get_ids().to_vec()).collect();
            let mask: Vec<Vec<u32>> = enc
                .iter()
                .map(|e| e.get_attention_mask().to_vec())
                .collect();
            let ids = Tensor::new(ids, &Device::Cpu)?;
            let mask = Tensor::new(mask, &Device::Cpu)?;
            let hidden = self.model.forward(&ids, &ids.zeros_like()?, Some(&mask))?;
            let pooled = match self.spec.pooling {
                Pooling::Cls => hidden.narrow(1, 0, 1)?.squeeze(1)?,
                Pooling::Mean => {
                    let m = mask.to_dtype(DType::F32)?.unsqueeze(2)?;
                    hidden
                        .broadcast_mul(&m)?
                        .sum(1)?
                        .broadcast_div(&m.sum(1)?)?
                }
            };
            let norm = pooled.sqr()?.sum_keepdim(D::Minus1)?.sqrt()?;
            out.extend(pooled.broadcast_div(&norm)?.to_vec2::<f32>()?);
        }
        Ok(out)
    }
}

impl Embedder for BertEmbedder {
    fn dim(&self) -> usize {
        self.spec.dim
    }

    fn model_id(&self) -> &str {
        self.spec.id
    }

    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RagError> {
        let p = self.spec.passage_prefix;
        self.embed(texts.iter().map(|t| format!("{p}{t}")).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, RagError> {
        let q = format!("{}{text}", self.spec.query_prefix);
        Ok(self.embed(vec![q])?.pop().unwrap_or_default())
    }
}

/// Counts what `embed_passages` feeds the model: prefix, text, and special tokens. So a chunk
/// within the chunker's `max_tokens` (512) is never truncated.
impl TokenCounter for BertEmbedder {
    fn count(&self, text: &str) -> usize {
        let t = format!("{}{text}", self.spec.passage_prefix);
        // shortcut: a tokenizer error counts as over any limit, so the chunker splits further; surface it if one ever shows up.
        self.tokenizer
            .encode_fast(t, true)
            .map_or(usize::MAX, |e| e.len())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Deterministic stand-in for CI: each lowercase word adds +-1 to one of 384 buckets chosen
/// by its sha256, then the vector is L2-normalized. Texts that share words come out similar.
pub struct FakeEmbedder;

impl FakeEmbedder {
    fn vector(text: &str) -> Vec<f32> {
        let mut v = vec![0f32; 384];
        for w in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            let h = Sha256::digest(w.to_lowercase().as_bytes());
            let i = usize::from(u16::from_le_bytes([h[0], h[1]])) % v.len();
            v[i] += if h[2] & 1 == 0 { 1.0 } else { -1.0 };
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm == 0.0 {
            v[0] = 1.0;
            return v;
        }
        v.iter().map(|x| x / norm).collect()
    }
}

impl Embedder for FakeEmbedder {
    fn dim(&self) -> usize {
        384
    }

    fn model_id(&self) -> &str {
        "fake"
    }

    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RagError> {
        Ok(texts.iter().map(|t| Self::vector(t)).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, RagError> {
        Ok(Self::vector(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const QUERY: &str = "contract termination clause";
    const RELATED: &str = "Either party may end this agreement with thirty days written notice if the other party materially breaches the contract.";
    const UNRELATED: &str =
        "Hold the small button on the back of the router for ten seconds until the lights blink.";

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    fn norm(v: &[f32]) -> f32 {
        dot(v, v).sqrt()
    }

    #[test]
    fn fake_vectors_are_unit_deterministic_and_word_based() {
        let e = FakeEmbedder;
        let texts = [
            "Contract termination",
            "",
            "!!!",
            "termination of the contract",
        ];
        let a = e.embed_passages(&texts).unwrap();
        assert_eq!(a, e.embed_passages(&texts).unwrap());
        for v in &a {
            assert_eq!(v.len(), e.dim());
            assert!((norm(v) - 1.0).abs() < 1e-5);
        }
        assert_eq!(a[0], e.embed_query("contract TERMINATION").unwrap());
        let q = e.embed_query(QUERY).unwrap();
        let p = e.embed_passages(&[texts[3], UNRELATED]).unwrap();
        assert!(dot(&q, &p[0]) > dot(&q, &p[1]));
    }

    /// Writes `{dir}/m/{id}/` with the given files and a manifest of their real hashes,
    /// then applies `tamper`.
    fn model_dir(
        id: &str,
        files: &[(&str, &[u8])],
        tamper: impl FnOnce(&Path),
    ) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path().join(id);
        std::fs::create_dir_all(&m).unwrap();
        let mut hashes = HashMap::new();
        for (name, data) in files {
            std::fs::write(m.join(name), data).unwrap();
            hashes.insert(*name, hex(&Sha256::digest(data)));
        }
        let manifest = serde_json::json!({ "files": hashes });
        std::fs::write(m.join("manifest.json"), manifest.to_string()).unwrap();
        tamper(&m);
        dir
    }

    const JUNK: &[(&str, &[u8])] = &[
        ("config.json", b"{}"),
        ("tokenizer.json", b"{}"),
        ("model.safetensors", b"not weights"),
    ];

    #[test]
    fn unknown_model_is_rejected() {
        let d = model_dir("x", JUNK, |_| {});
        assert!(matches!(
            BertEmbedder::load(d.path(), "x"),
            Err(RagError::UnknownModel)
        ));
    }

    #[test]
    fn a_file_that_differs_from_its_manifest_is_never_parsed() {
        let id = MODELS[0].id;
        let d = model_dir(id, JUNK, |m| {
            std::fs::write(m.join("model.safetensors"), b"swapped").unwrap()
        });
        assert!(matches!(
            BertEmbedder::load(d.path(), id),
            Err(RagError::HashMismatch("model.safetensors"))
        ));
        // Same junk with matching hashes gets past the check and fails in parsing instead.
        let d = model_dir(id, JUNK, |_| {});
        assert!(!matches!(
            BertEmbedder::load(d.path(), id),
            Err(RagError::HashMismatch(_))
        ));
    }

    #[test]
    fn a_file_missing_from_the_manifest_is_a_mismatch() {
        let id = MODELS[0].id;
        let d = model_dir(id, &JUNK[..2], |m| {
            std::fs::write(m.join("model.safetensors"), b"x").unwrap()
        });
        assert!(matches!(
            BertEmbedder::load(d.path(), id),
            Err(RagError::HashMismatch("model.safetensors"))
        ));
    }

    /// `LOCITE_MODELS_DIR`, else `<repo>/models`.
    fn real(id: &str) -> BertEmbedder {
        let dir = std::env::var_os("LOCITE_MODELS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../models"));
        BertEmbedder::load(&dir, id).unwrap()
    }

    #[test]
    #[ignore = "needs model weights: scripts/fetch-models.sh"]
    fn real_models_rank_related_text_above_unrelated() {
        for spec in MODELS {
            let e = real(spec.id);
            let q = e.embed_query(QUERY).unwrap();
            let p = e.embed_passages(&[RELATED, UNRELATED]).unwrap();
            assert_eq!(q.len(), e.dim());
            assert!((norm(&q) - 1.0).abs() < 1e-4);
            assert!(dot(&q, &p[0]) > dot(&q, &p[1]), "{}", spec.id);
        }
    }

    #[test]
    #[ignore = "needs model weights: scripts/fetch-models.sh"]
    fn real_batches_match_single_embeddings_and_counts_fit() {
        let e = real(MODELS[0].id);
        let texts: Vec<String> = (0..BATCH + 3).map(|i| format!("{UNRELATED} {i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let all = e.embed_passages(&refs).unwrap();
        assert_eq!(all.len(), texts.len());
        let last = e.embed_passages(&refs[BATCH + 2..]).unwrap();
        assert!(dot(&all[BATCH + 2], &last[0]) > 0.9999);
        // [CLS] + 2 words + [SEP].
        assert_eq!(e.count("hello world"), 4);
    }
}
