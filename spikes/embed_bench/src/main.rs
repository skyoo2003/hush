//! Throwaway spike for ADR-013: time 1,000 chunks per model and device, plus a tiny
//! English retrieval check. Run from the repo root:
//! `cargo run --release --manifest-path spikes/embed_bench/Cargo.toml -- models`

use std::path::Path;
use std::time::Instant;

use candle_core::{DType, Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::{bert, nomic_bert};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

enum Net {
    Bert(bert::BertModel),
    Nomic(nomic_bert::NomicBertModel),
}

struct Spec {
    id: &'static str,
    nomic: bool,
    cls: bool,
    query: &'static str,
    passage: &'static str,
}

const SPECS: &[Spec] = &[
    Spec { id: "bge-small-en-v1.5", nomic: false, cls: true, query: "Represent this sentence for searching relevant passages: ", passage: "" },
    Spec { id: "multilingual-e5-small", nomic: false, cls: false, query: "query: ", passage: "passage: " },
    Spec { id: "nomic-embed-text-v1.5", nomic: true, cls: false, query: "search_query: ", passage: "search_document: " },
];

struct Model {
    net: Net,
    tok: Tokenizer,
    cls: bool,
    dev: Device,
}

fn load(dir: &Path, s: &Spec, dev: &Device) -> Model {
    let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[dir.join("model.safetensors")], DType::F32, dev).unwrap()
    };
    let net = if s.nomic {
        Net::Nomic(nomic_bert::NomicBertModel::load(vb, &serde_json::from_str(&cfg).unwrap()).unwrap())
    } else {
        Net::Bert(bert::BertModel::load(vb, &serde_json::from_str(&cfg).unwrap()).unwrap())
    };
    let mut tok = Tokenizer::from_file(dir.join("tokenizer.json")).unwrap();
    tok.with_padding(Some(PaddingParams::default()));
    tok.with_truncation(Some(TruncationParams { max_length: 512, ..Default::default() })).unwrap();
    Model { net, tok, cls: s.cls, dev: dev.clone() }
}

impl Model {
    fn embed(&self, texts: &[String]) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(32) {
            let enc = self.tok.encode_batch(batch.to_vec(), true).unwrap();
            let ids: Vec<Vec<u32>> = enc.iter().map(|e| e.get_ids().to_vec()).collect();
            let mask: Vec<Vec<u32>> = enc.iter().map(|e| e.get_attention_mask().to_vec()).collect();
            let ids = Tensor::new(ids, &self.dev).unwrap();
            let mask = Tensor::new(mask, &self.dev).unwrap();
            let types = ids.zeros_like().unwrap();
            let h = match &self.net {
                Net::Bert(m) => m.forward(&ids, &types, Some(&mask)).unwrap(),
                Net::Nomic(m) => m.forward(&ids, Some(&types), Some(&mask)).unwrap(),
            };
            let pooled = if self.cls {
                h.narrow(1, 0, 1).unwrap().squeeze(1).unwrap()
            } else {
                nomic_bert::mean_pooling(&h, &mask).unwrap()
            };
            let norm = pooled.sqr().unwrap().sum_keepdim(D::Minus1).unwrap().sqrt().unwrap();
            let v = pooled.broadcast_div(&norm).unwrap().to_vec2::<f32>().unwrap();
            out.extend(v);
        }
        out
    }
}

/// 1,000 synthetic English passages of ~1,600 chars, the chunker's 400-token target under CharApprox.
fn corpus() -> Vec<String> {
    let subj = ["The supplier", "Each party", "The tenant", "Our team", "The committee", "The vendor", "The licensee"];
    let verb = ["shall deliver", "reviewed", "must report", "agreed to extend", "will measure", "documented", "refused to sign"];
    let obj = ["the quarterly invoice", "the safety inspection results", "a revised project schedule", "the confidential appendix", "the warranty claim", "the onboarding checklist", "all outstanding payments"];
    let tail = ["before the end of the month.", "according to section 4.2.", "without prior written notice.", "after the audit concluded.", "as described in the attached memo.", "unless both parties agree otherwise."];
    (0..1000)
        .map(|i| {
            let mut s = String::new();
            let mut k = i as u64;
            while s.len() < 1600 {
                k = k.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let r = (k >> 33) as usize;
                s.push_str(&format!("{} {} {} {} ", subj[r % 7], verb[(r / 7) % 7], obj[(r / 49) % 7], tail[(r / 343) % 6]));
            }
            s
        })
        .collect()
}

/// (query, relevant passage). Passages share vocabulary on purpose.
const PAIRS: &[(&str, &str)] = &[
    ("contract termination clause", "Either party may end this agreement with thirty days written notice if the other party materially breaches its obligations."),
    ("how long is the warranty", "The manufacturer covers defects in materials and workmanship for twenty-four months from the date of purchase."),
    ("late payment penalty", "Invoices not settled within forty-five days accrue interest at one and a half percent per month."),
    ("who owns the intellectual property", "All inventions, designs, and source code created under this agreement belong exclusively to the client."),
    ("confidentiality obligations after the contract ends", "The duty to protect secret information survives for five years after this agreement expires."),
    ("governing law and jurisdiction", "Any dispute will be resolved by the courts of the State of New York, whose laws apply to this agreement."),
    ("limitation of liability", "Neither party is liable for indirect or consequential damages, and total damages are capped at the fees paid."),
    ("employee vacation policy", "Full-time staff accrue fifteen days of paid leave each year, and up to five unused days roll over."),
    ("remote work rules", "Employees may work from home up to three days per week with approval from their manager."),
    ("expense reimbursement deadline", "Submit receipts for travel costs within thirty days of the trip to be repaid."),
    ("password requirements", "Account secrets must be at least twelve characters long and rotated every ninety days."),
    ("data retention period", "Customer records are kept for seven years and then securely destroyed."),
    ("how to reset the router", "Hold the small button on the back of the device for ten seconds until the lights blink."),
    ("battery replacement instructions", "Remove the four screws on the rear panel, disconnect the old cell, and insert the new one."),
    ("side effects of the medication", "Some patients report drowsiness, dry mouth, and mild headaches during the first week of treatment."),
    ("recommended dosage for adults", "Grown patients should take one 200 mg tablet twice a day with food."),
    ("quarterly revenue growth", "Sales rose eleven percent compared with the same three months last year."),
    ("reasons the project was delayed", "The schedule slipped because the supplier shipped the steel beams six weeks late."),
    ("fire evacuation procedure", "When the alarm sounds, leave by the nearest stairwell and gather in the north parking lot."),
    ("refund policy for cancelled orders", "Purchases cancelled before shipment receive their money back within five business days."),
    ("non-compete restriction", "For one year after leaving, the employee may not join a direct competitor in the same region."),
    ("security deposit return", "The landlord returns the tenant's deposit within twenty-one days after move-out, minus repair costs."),
    ("rent increase notice", "The monthly lease amount may rise only once a year, with sixty days advance notice to the tenant."),
    ("force majeure events", "Neither side is responsible for failures caused by floods, wars, pandemics, or other events beyond its control."),
];

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| "models".into());
    let corpus = corpus();
    let mut devices = vec![("cpu", Device::Cpu)];
    if let (Ok(m), None) = (Device::new_metal(0), std::env::var_os("NO_METAL")) {
        devices.push(("metal", m));
    }
    for s in SPECS {
        let dir = Path::new(&root).join(s.id);
        for (dname, dev) in &devices {
            let t = Instant::now();
            let m = load(&dir, s, dev);
            let load_ms = t.elapsed().as_millis();
            let docs: Vec<String> = corpus.iter().map(|c| format!("{}{}", s.passage, c)).collect();
            let tokens: usize = docs.iter().map(|d| m.tok.encode(d.as_str(), true).unwrap().len()).sum();
            m.embed(&docs[..32]); // warm-up
            let mut runs = Vec::new();
            for _ in 0..3 {
                let t = Instant::now();
                let v = m.embed(&docs);
                assert_eq!(v.len(), 1000);
                runs.push(t.elapsed().as_secs_f64());
            }
            println!(
                "{:24} {:5} load {:5} ms  1000 chunks: {:.2} / {:.2} / {:.2} s  avg tokens {}",
                s.id, dname, load_ms, runs[0], runs[1], runs[2], tokens / 1000
            );
            if *dname != "cpu" {
                continue;
            }
            let qs: Vec<String> = PAIRS.iter().map(|p| format!("{}{}", s.query, p.0)).collect();
            let ps: Vec<String> = PAIRS.iter().map(|p| format!("{}{}", s.passage, p.1)).collect();
            let (qv, pv) = (m.embed(&qs), m.embed(&ps));
            let (mut r1, mut mrr) = (0, 0.0);
            for (i, q) in qv.iter().enumerate() {
                let mine = dot(q, &pv[i]);
                let rank = 1 + pv.iter().filter(|p| dot(q, p) > mine).count();
                r1 += (rank == 1) as usize;
                mrr += 1.0 / rank as f64;
            }
            let related = dot(&qv[0], &pv[0]);
            let unrelated = dot(&qv[0], &pv[12]);
            println!(
                "{:24} quality  R@1 {}/{}  MRR {:.3}  termination: related {:.3} unrelated {:.3}",
                s.id, r1, PAIRS.len(), mrr / PAIRS.len() as f64, related, unrelated
            );
        }
    }
}
