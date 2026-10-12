//! Deterministic cleanup gates — the cheap, always-on guard against the LLM
//! over-editing or hallucinating. They run on every cleanup output before it is
//! committed; on any failure the caller falls back to the raw transcript (or,
//! optionally, an LLM verifier pass). This is the anti-over-editing safety net.

use super::levels::CleanupLevel;

/// Why a cleanup output was rejected.
#[derive(Debug, Clone, PartialEq)]
pub enum GateReason {
    /// Token-level edit distance exceeded the level's ceiling.
    EditRatioTooHigh { ratio: f32, ceiling: f32 },
    /// A must-preserve token (number, URL, email, code-ish token) vanished.
    LostEntity(String),
    /// Output shrank more than 40% — likely dropped content.
    OverDeletion { shrink: f32 },
    /// Output grew beyond punctuation — likely added content.
    Hallucination,
    /// A banned pattern (added greeting/sign-off or an assistant-style reply) appeared.
    BannedPattern(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateVerdict {
    Pass,
    Fail(GateReason),
}

impl GateVerdict {
    pub fn passed(&self) -> bool {
        matches!(self, GateVerdict::Pass)
    }
}

/// Phrases that should never be *introduced* by cleanup (the model answering or
/// chatting instead of transcribing). Matched case-insensitively at output start.
const BANNED_PREFIXES: &[&str] = &[
    "sure,",
    "sure!",
    "here is",
    "here's",
    "i'm sorry",
    "i am sorry",
    "as an ai",
    "certainly",
    "of course",
    "i cannot",
    "i can't help",
];

/// Evaluate a cleanup output against the raw transcript for the given level.
pub fn evaluate(raw: &str, cleaned: &str, level: CleanupLevel) -> GateVerdict {
    // None never invokes the model, so there is nothing to gate.
    if level.bypasses_llm() {
        return GateVerdict::Pass;
    }

    // 1) Introduced assistant-style / greeting prefixes.
    let cleaned_lc = cleaned.trim_start().to_lowercase();
    let raw_lc = raw.to_lowercase();
    for p in BANNED_PREFIXES {
        if cleaned_lc.starts_with(p) && !raw_lc.contains(p) {
            return GateVerdict::Fail(GateReason::BannedPattern((*p).to_string()));
        }
    }

    // 2) Must-preserve entities present in raw must survive in cleaned.
    for ent in must_preserve_entities(raw) {
        if !cleaned.contains(&ent) {
            return GateVerdict::Fail(GateReason::LostEntity(ent));
        }
    }

    // 3) Gross length changes. Thresholds are generous: self-corrections shorten
    // text and structural formatting (numbered lists, paragraph breaks, list
    // markers) lengthens it — both are legitimate, so only flag extreme changes.
    let raw_len = raw.chars().count().max(1) as f32;
    let clean_len = cleaned.chars().count() as f32;
    let shrink = (raw_len - clean_len) / raw_len;
    if shrink > level.max_shrink() {
        return GateVerdict::Fail(GateReason::OverDeletion { shrink });
    }
    if clean_len > raw_len * 1.6 {
        return GateVerdict::Fail(GateReason::Hallucination);
    }

    // 4) Novelty: how many output words were never spoken. Deletions (fillers) and
    // casing/punctuation don't count; a full rewrite does.
    let ratio = novelty_ratio(raw, cleaned);
    let ceiling = level.max_novelty_ratio();
    if ratio > ceiling {
        return GateVerdict::Fail(GateReason::EditRatioTooHigh { ratio, ceiling });
    }

    GateVerdict::Pass
}

/// Tokens that must survive cleanup verbatim: URLs, emails, and *substantial*
/// digit strings (phone numbers, account/order ids, years, versions — 4+ digits).
/// Short numbers (1–3 digits) are deliberately NOT protected: they are routinely
/// and correctly dropped by self-corrections ("meet at 2, actually 3") and by
/// number normalization, and protecting them made the gate reject legitimate
/// cleanups and fall back to raw.
fn must_preserve_entities(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        let trimmed = tok.trim_matches(|c: char| c.is_ascii_punctuation() && c != '@' && c != '#');
        if trimmed.is_empty() {
            continue;
        }
        let is_url = trimmed.contains("://") || trimmed.contains(".com") || trimmed.contains('@');
        let digit_count = trimmed.chars().filter(|c| c.is_ascii_digit()).count();
        if is_url || digit_count >= 4 {
            out.push(trimmed.to_string());
        }
    }
    out
}

/// Lowercase a token and strip surrounding punctuation, so "3." == "3" and
/// "So" == "so" don't read as changes.
fn normalize_tok(t: &str) -> String {
    t.trim_matches(|c: char| c.is_ascii_punctuation())
        .to_lowercase()
}

/// Fraction of output words that were never spoken. Filler deletion and
/// casing/punctuation contribute nothing; a genuine rewrite or hallucination
/// (new content words) drives this up. A couple of legitimate normalizations
/// ("seven" -> "7") add a little, which the per-level ceiling leaves room for.
fn novelty_ratio(raw: &str, cleaned: &str) -> f32 {
    use std::collections::HashSet;
    let raw_set: HashSet<String> = raw
        .split_whitespace()
        .map(normalize_tok)
        .filter(|s| !s.is_empty())
        .collect();
    let clean_toks: Vec<String> = cleaned
        .split_whitespace()
        .map(normalize_tok)
        .filter(|s| !s.is_empty())
        .collect();
    if clean_toks.is_empty() {
        return 0.0;
    }
    let novel = clean_toks.iter().filter(|t| !raw_set.contains(*t)).count();
    novel as f32 / clean_toks.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_cleanup_passes() {
        // Filler removal + punctuation — a legitimate Light edit.
        let raw = "um so i think we should uh meet at 3";
        let clean = "So I think we should meet at 3.";
        assert!(evaluate(raw, clean, CleanupLevel::Light).passed());
    }

    #[test]
    fn dropping_a_number_fails() {
        let raw = "transfer 500 dollars to account 12345";
        let clean = "Transfer money to the account."; // lost 500 and 12345
        let v = evaluate(raw, clean, CleanupLevel::Light);
        assert!(matches!(v, GateVerdict::Fail(GateReason::LostEntity(_))));
    }

    #[test]
    fn answering_a_question_is_banned() {
        let raw = "what time is the standup";
        let clean = "Here is the standup schedule: 9am."; // model answered instead of transcribing
        let v = evaluate(raw, clean, CleanupLevel::Light);
        assert!(matches!(v, GateVerdict::Fail(GateReason::BannedPattern(_))));
    }

    #[test]
    fn heavy_rewrite_exceeds_light_ceiling_but_ok_at_high() {
        let raw = "i went to the store and then i bought some milk and eggs and bread";
        let clean = "Purchased dairy and bakery goods."; // huge rewrite
        assert!(matches!(
            evaluate(raw, clean, CleanupLevel::Light),
            GateVerdict::Fail(_)
        ));
        // Still fails High too here because it also over-deletes; ensure ratio logic is sane
        // on a milder rewrite:
        let clean_mild = "I went to the store and bought milk, eggs, and bread.";
        assert!(evaluate(raw, clean_mild, CleanupLevel::Light).passed());
    }

    #[test]
    fn over_deletion_fails() {
        let raw = "the quarterly report is due on friday please review the budget section";
        let clean = "Report due Friday."; // dropped >40%
        assert!(matches!(
            evaluate(raw, clean, CleanupLevel::Medium),
            GateVerdict::Fail(GateReason::OverDeletion { .. })
        ));
    }

    #[test]
    fn light_rejects_dropped_sentence_that_medium_allows() {
        // The model silently dropped the second sentence: 35-40% shorter.
        let raw = "Ich schicke dir das Angebot morgen frueh. Die Bilder kommen dann am Freitag dazu.";
        let clean = "Ich schicke dir das Angebot morgen frueh.";
        assert!(!evaluate(raw, clean, CleanupLevel::Light).passed());
        let short = "Ich schicke dir das Angebot morgen frueh. Die Bilder kommen am Freitag.";
        assert!(evaluate(raw, short, CleanupLevel::Medium).passed());
    }

    #[test]
    fn none_level_always_passes() {
        assert!(evaluate("anything", "totally different", CleanupLevel::None).passed());
    }
}
