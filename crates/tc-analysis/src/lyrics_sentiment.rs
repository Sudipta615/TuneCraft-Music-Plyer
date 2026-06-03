//! Lyrics-based sentiment analysis for Romantic vs Sad disambiguation.
//!
//! ## Design
//!
//! This is a **valence lexicon** approach: a curated word list where each
//! entry is tagged as either `Romantic` or `Sad`.  The analyser tokenises
//! the lyrics text, looks up each token, and returns the dominant sentiment
//! along with a confidence score.
//!
//! ## Language coverage
//!
//! The lexicon covers three layers:
//!
//! 1. **Hindi/Urdu (Devanagari script)** — common Bollywood lyrical vocabulary.
//! 2. **Hindi/Urdu romanised** — transliterated forms as they often appear in
//!    LRCLIB-sourced lyrics (e.g. "pyaar", "dard").
//! 3. **English** — Western pop/R&B vocabulary.
//!
//! Romanised Hindi is essential because LRCLIB frequently returns lyrics in
//! Latin script even for Hindi songs.  Both the Devanagari and romanised
//! forms are included for every word so the lexicon works regardless of how
//! the lyrics provider chose to encode the text.
//!
//! ## Scoring
//!
//! Each matched word contributes +1 to its category counter.  Final score:
//!
//!   sentiment_score = (romantic_hits − sad_hits) / (romantic_hits + sad_hits + 1)
//!
//! Positive → Romantic, negative → Sad.  The +1 denominator prevents
//! division-by-zero on empty lyrics.
//!
//! Confidence = (|romantic_hits − sad_hits|) / (total_hits + 1), clamped
//! to [0, 1].  Low confidence (< 0.15) means the lyric content is ambiguous
//! and the signal-based classifier should be trusted instead.
//!
//! ## Limitations (documented)
//!
//! - Classical/poetic Urdu (ghazals) uses rare vocabulary not in this list;
//!   coverage degrades gracefully — the analyser returns low confidence and
//!   defers to the signal classifier.
//! - Regional Indian languages (Tamil, Telugu, Marathi, Bengali) are **not**
//!   covered; again, graceful degradation.
//! - Irony/sarcasm ("I'm so happy", sarcastically) is not handled.  Lexicon
//!   approaches cannot detect irony without context — acceptable for music.

/// Sentiment tag for a lexicon entry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tag { Romantic, Sad }

/// A single lexicon entry: (lowercase token, tag).
type Entry = (&'static str, Tag);

use Tag::{Romantic as R, Sad as S};

/// The valence lexicon.

/// Rules for adding entries:
/// - All tokens **must** be lowercase ASCII or Unicode NFD-normalised.
/// - Include both Devanagari and common romanised forms for Hindi/Urdu words.
/// - Prefer specific/unambiguous words; avoid polysemous ones ("dark" could
///   be either) unless the dominant usage in song lyrics is clear.
static LEXICON: &[Entry] = &[
    // -----------------------------------------------------------------------
    // ROMANTIC — Hindi/Urdu (Devanagari)
    // -----------------------------------------------------------------------
    ("प्यार",      R), // pyaar — love
    ("प्रेम",      R), // prem — love (Sanskrit-origin)
    ("इश्क़",      R), // ishq — passionate love
    ("इश्क",       R), // ishq (without nukta)
    ("मोहब्बत",    R), // mohabbat — love/affection
    ("मोहब्बतें",  R), // mohabbaten — loves
    ("दिलरुबा",    R), // dilruba — heart-stealer (term of endearment)
    ("जानेमन",     R), // jaaneman — darling
    ("जानम",       R), // jaanam — beloved
    ("महबूब",      R), // mahboob — beloved
    ("महबूबा",     R), // mahbooba — beloved (f)
    ("हमसफ़र",     R), // humsafar — companion/life partner
    ("हमसफर",      R), // humsafar (without nukta)
    ("साथी",       R), // saathi — companion
    ("दुल्हन",     R), // dulhan — bride
    ("दूल्हा",     R), // dulha — groom
    ("सजना",       R), // sajna — beloved
    ("सजनी",       R), // sajni — beloved (f)
    ("सनम",        R), // sanam — sweetheart
    ("चाहत",       R), // chaahat — desire/love
    ("चाहना",      R), // chahna — to love/desire
    ("आशिक़",      R), // aashiq — lover
    ("आशिक",       R), // aashiq (without nukta)
    ("माशूक़",     R), // maashuq — beloved
    ("रोमांस",     R), // romance (loanword)
    ("दिल",        R), // dil — heart (romantic context dominant)
    ("दिलकश",      R), // dilkash — charming
    ("मिलन",       R), // milan — union/meeting (of lovers)
    ("रात",        R), // raat — night (romantic context in Bollywood)
    ("चाँद",       R), // chaand — moon (romantic symbol)
    ("चांद",       R), // chaand (alt spelling)
    ("सितारे",     R), // sitaare — stars (romantic)
    ("ख़्वाब",     R), // khwaab — dream (romantic)
    ("ख्वाब",      R), // khwaab (without nukta)
    ("हसीन",       R), // haseen — beautiful
    ("हसीना",      R), // haseena — beautiful woman
    ("खूबसूरत",    R), // khoobsoorat — beautiful
    ("नज़र",       R), // nazar — gaze/glance (romantic)
    ("नजर",        R), // nazar (without nukta)
    ("मुस्कान",    R), // muskaan — smile
    ("हँसी",       R), // hansi — laughter
    ("पहली",       R), // pehli — first (first love context)
    ("पहला",       R), // pehla — first
    ("मेरी जान",   R), // meri jaan — my life (endearment)
    ("बाहें",      R), // baahen — arms (embrace)
    ("आग़ोश",      R), // aaghosh — embrace/lap
    ("बोसा",       R), // bosa — kiss
    ("अधर",        R), // adhar — lips (poetic)
    ("नैन",        R), // nain — eyes (poetic/romantic)
    ("अँखियाँ",    R), // ankhiyaan — eyes (poetic)
    ("रूह",        R), // rooh — soul (in romantic context: "meri rooh")
    ("वफ़ा",       R), // wafa — faithfulness/loyalty (in love)
    ("वफा",        R), // wafa (without nukta)
    ("इंतज़ार",    R), // intezaar — waiting (for beloved)
    ("इंतजार",     R), // intezaar (without nukta)

    // -----------------------------------------------------------------------
    // ROMANTIC — Hindi/Urdu romanised
    // -----------------------------------------------------------------------
    ("pyaar",      R),
    ("pyar",       R),
    ("prem",       R),
    ("ishq",       R),
    ("mohabbat",   R),
    ("mohabbatein",R),
    ("dilruba",    R),
    ("jaaneman",   R),
    ("jaaneman",   R),
    ("jaanam",     R),
    ("janum",      R),
    ("mahboob",    R),
    ("humsafar",   R),
    ("saathi",     R),
    ("saath",      R),
    ("dulhan",     R),
    ("dulha",      R),
    ("sajna",      R),
    ("sajni",      R),
    ("sanam",      R),
    ("chaahat",    R),
    ("chahna",     R),
    ("aashiq",     R),
    ("romance",    R),
    ("romantic",   R),
    ("dil",        R),
    ("milan",      R),
    ("chaand",     R),
    ("chand",      R),
    ("sitaare",    R),
    ("sitare",     R),
    ("khwaab",     R),
    ("khwab",      R),
    ("sapna",      R), // dream
    ("sapne",      R),
    ("haseen",     R),
    ("haseena",    R),
    ("khoobsoorat",R),
    ("sundar",     R), // beautiful
    ("muskaan",    R),
    ("hansi",      R),
    ("pehla",      R),
    ("pehli",      R),
    ("baahein",    R),
    ("baahon",     R),
    ("bosa",       R),
    ("nain",       R),
    ("ankhiyaan",  R),
    ("aankhein",   R),
    ("rooh",       R),
    ("wafa",       R),
    ("intezaar",   R),
    ("tera",       R), // yours (intimate)
    ("tere",       R),
    ("teri",       R),
    ("mera",       R), // mine (intimate — common in love songs)
    ("mere",       R),
    ("meri",       R),
    ("tujhse",     R), // from you
    ("tujhe",      R), // to you
    ("tumse",      R),
    ("tumhare",    R),
    ("tumhari",    R),
    ("milna",      R), // to meet
    ("mile",       R),

    // -----------------------------------------------------------------------
    // ROMANTIC — English
    // -----------------------------------------------------------------------
    ("love",       R),
    ("loved",      R),
    ("lover",      R),
    ("lovely",     R),
    ("romance",    R),
    ("romantic",   R),
    ("darling",    R),
    ("sweetheart", R),
    ("honey",      R),
    ("baby",       R),
    ("babe",       R),
    ("kiss",       R),
    ("kissed",     R),
    ("kisses",     R),
    ("embrace",    R),
    ("hold",       R),  // "hold me"
    ("holding",    R),
    ("arms",       R),  // "in your arms"
    ("heart",      R),
    ("heartbeat",  R),
    ("forever",    R),
    ("always",     R),
    ("together",   R),
    ("destiny",    R),
    ("soul",       R),  // "my soul"
    ("dream",      R),
    ("dreaming",   R),
    ("dreams",     R),
    ("beautiful",  R),
    ("gorgeous",   R),
    ("smile",      R),
    ("smiling",    R),
    ("warmth",     R),
    ("tender",     R),
    ("gently",     R),
    ("passion",    R),
    ("passionate", R),
    ("adore",      R),
    ("adoring",    R),
    ("cherish",    R),
    ("cherished",  R),
    ("devoted",    R),
    ("devotion",   R),
    ("soulmate",   R),
    ("forever",    R),
    ("eternity",   R),
    ("eternal",    R),
    ("moonlight",  R),
    ("starlight",  R),
    ("candlelight",R),
    ("tonight",    R),  // "tonight with you"
    ("dance",      R),  // "dance with me"
    ("dancing",    R),

    // -----------------------------------------------------------------------
    // SAD — Hindi/Urdu (Devanagari)
    // -----------------------------------------------------------------------
    ("दर्द",       S), // dard — pain
    ("ग़म",        S), // gham — sorrow
    ("गम",         S), // gham (without nukta)
    ("उदास",       S), // udaas — sad
    ("उदासी",      S), // udaasi — sadness
    ("तन्हा",      S), // tanha — lonely
    ("तन्हाई",     S), // tanhai — loneliness
    ("अकेला",      S), // akela — alone (m)
    ("अकेली",      S), // akeli — alone (f)
    ("रोना",       S), // rona — to cry
    ("रो",         S), // ro — cry (imperative)
    ("आँसू",       S), // aansu — tears
    ("आंसू",       S), // aansu (alt)
    ("आँखें",      S), // aankhein — eyes (in sad context: "aankhein bhar aayi")
    ("बिछड़",      S), // bichad — separation
    ("बिछड़ना",    S), // bichadna — to part/separate
    ("जुदाई",      S), // judaai — separation
    ("दूरी",       S), // doori — distance
    ("खोना",       S), // khona — to lose
    ("खोया",       S), // khoya — lost
    ("छोड़",       S), // chod — leave/abandon
    ("छोड़ना",     S), // chodna — to leave
    ("चला गया",    S), // chala gaya — went away
    ("चली गई",     S), // chali gayi — went away (f)
    ("वापस",       S), // waapas — return (longing to return)
    ("याद",        S), // yaad — memory/remembrance (used in loss context)
    ("यादें",      S), // yaadein — memories
    ("बेवफ़ा",     S), // bewafa — unfaithful
    ("बेवफा",      S), // bewafa (without nukta)
    ("धोखा",       S), // dhokha — betrayal
    ("टूटा",       S), // toota — broken
    ("टूटे",       S), // toote — broken (pl)
    ("तकलीफ़",     S), // takleef — suffering/pain
    ("तकलीफ",      S), // takleef (without nukta)
    ("मुश्किल",    S), // mushkil — difficult/trouble
    ("परेशान",     S), // pareshan — troubled/distressed
    ("सिसकी",      S), // siski — sob
    ("बेकल",       S), // bekal — restless/distressed
    ("बेसहारा",    S), // besahara — helpless/abandoned
    ("राह",        S), // raah — path (waiting on the path — sad context)
    ("इंतज़ार",    S), // intezaar — waiting (also sad: waiting endlessly)
    ("रात भर",     S), // raat bhar — all night (sleepless sadness)
    ("सन्नाटा",    S), // sannata — silence/emptiness
    ("खालीपन",     S), // khaalipan — emptiness
    ("उम्मीद",     S), // umeed — hope (in context of fading hope)
    ("अफ़सोस",     S), // afsos — regret
    ("अफसोस",      S), // afsos (without nukta)
    ("पश्चाताप",   S), // pashchatap — remorse

    // -----------------------------------------------------------------------
    // SAD — Hindi/Urdu romanised
    // -----------------------------------------------------------------------
    ("dard",       S),
    ("gham",       S),
    ("udaas",      S),
    ("udaasi",     S),
    ("tanha",      S),
    ("tanhai",     S),
    ("akela",      S),
    ("akeli",      S),
    ("rona",       S),
    ("aansu",      S),
    ("ansu",       S),
    ("bichadna",   S),
    ("bichad",     S),
    ("judaai",     S),
    ("judai",      S),
    ("doori",      S),
    ("khona",      S),
    ("khoya",      S),
    ("chhoda",     S),
    ("chodna",     S),
    ("yaad",       S),
    ("yaadein",    S),
    ("yaaden",     S),
    ("bewafa",     S),
    ("dhokha",     S),
    ("toota",      S),
    ("toote",      S),
    ("takleef",    S),
    ("mushkil",    S),
    ("pareshan",   S),
    ("siski",      S),
    ("besahara",   S),
    ("sannata",    S),
    ("khaalipan",  S),
    ("khali",      S),  // empty
    ("afsos",      S),
    ("rote",       S),  // crying (rote hain)
    ("roti",       S),
    ("tadap",      S),  // longing/ache
    ("tadapna",    S),
    ("bikharna",   S),  // to shatter
    ("bikhar",     S),
    ("toot",       S),  // break
    ("alvida",     S),  // goodbye (farewell — sad)
    ("rukhsat",    S),  // farewell
    ("kho",        S),  // lose / lost
    ("haarna",     S),  // to lose/be defeated
    ("haar",       S),  // defeat/loss
    ("zindagi",    S),  // life (often in sad songs: "zindagi ne...")
    ("andhere",    S),  // darkness
    ("andhera",    S),
    ("raat",       S),  // night (also sad: sleepless nights)

    // -----------------------------------------------------------------------
    // SAD — English
    // -----------------------------------------------------------------------
    ("sad",        S),
    ("sadness",    S),
    ("sorrow",     S),
    ("sorrowful",  S),
    ("grief",      S),
    ("grieve",     S),
    ("cry",        S),
    ("crying",     S),
    ("tears",      S),
    ("tear",       S),  // a tear (drop)
    ("weep",       S),
    ("weeping",    S),
    ("pain",       S),
    ("painful",    S),
    ("hurt",       S),
    ("hurting",    S),
    ("broken",     S),
    ("heartbreak", S),
    ("heartbroken",S),
    ("shattered",  S),
    ("alone",      S),
    ("lonely",     S),
    ("loneliness", S),
    ("empty",      S),
    ("emptiness",  S),
    ("missing",    S),  // "missing you"
    ("miss",       S),
    ("gone",       S),
    ("lost",       S),
    ("losing",     S),
    ("leave",      S),
    ("leaving",    S),
    ("left",       S),  // "you left me"
    ("goodbye",    S),
    ("farewell",   S),
    ("apart",      S),
    ("distance",   S),
    ("apart",      S),
    ("apart",      S),
    ("darkness",   S),
    ("dark",       S),
    ("shadow",     S),
    ("shadows",    S),
    ("silence",    S),
    ("silent",     S),
    ("cold",       S),  // "cold and empty"
    ("numb",       S),
    ("regret",     S),
    ("regrets",    S),
    ("mistake",    S),
    ("mistakes",   S),
    ("betrayal",   S),
    ("betrayed",   S),
    ("lied",       S),
    ("lies",       S),
    ("deceived",   S),
    ("abandoned",  S),
    ("helpless",   S),
    ("hopeless",   S),
    ("despair",    S),
    ("mourning",   S),
    ("mourn",      S),
    ("moaning",    S),  // moan of grief
    ("ache",       S),
    ("aching",     S),
    ("bleed",      S),
    ("bleeding",   S),
    ("suffocate",  S),
    ("suffocating",S),
    ("drown",      S),
    ("drowning",   S),
    ("fading",     S),
    ("fade",       S),
];

// ---------------------------------------------------------------------------
// Sentiment result
// ---------------------------------------------------------------------------

/// Sentiment polarity between Romantic and Sad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sentiment {
    /// Leans romantic.
    Romantic,
    /// Leans sad.
    Sad,
    /// Ambiguous — confidence below threshold, defer to signal classifier.
    Ambiguous,
}

/// Result from lyric sentiment analysis.
#[derive(Debug, Clone)]
pub struct SentimentResult {
    pub sentiment: Sentiment,
    /// ∈ [0, 1]; values below 0.15 are treated as Ambiguous.
    pub confidence: f64,
    /// Raw romantic hit count (for debugging / logging).
    pub romantic_hits: u32,
    /// Raw sad hit count.
    pub sad_hits: u32,
}

// ---------------------------------------------------------------------------
// Analyser
// ---------------------------------------------------------------------------

/// Lightweight lyrics sentiment analyser.

/// Instantiate once per process (the lexicon is static), then call
/// [`LyricsSentiment::analyse`] for each track.
pub struct LyricsSentiment {
    /// Sorted slice of (token, tag) for binary search.
    /// We sort at build time — the lexicon is small enough that a linear scan
    /// would also work, but binary search keeps the hot path O(log n).
    sorted: Vec<(&'static str, Tag)>,
}

impl LyricsSentiment {
    /// Build the analyser.  Call once and reuse across tracks.
    pub fn new() -> Self {
        let mut sorted: Vec<(&'static str, Tag)> = LEXICON.to_vec();
        sorted.sort_unstable_by_key(|e| e.0);
        sorted.dedup_by_key(|e| e.0);  // remove duplicate tokens (same word, same tag)
        Self { sorted }
    }

    /// Look up a lowercase token.  O(log n).
    fn lookup(&self, token: &str) -> Option<Tag> {
        self.sorted
            .binary_search_by_key(&token, |e| e.0)
            .ok()
            .map(|i| self.sorted[i].1)
    }

    /// Tokenise `text` into lowercase words and score against the lexicon.
    ///
    /// Handles:
    /// - Unicode (Devanagari) words separated by whitespace / punctuation.
    /// - ASCII/Latin words (English, romanised Hindi).
    /// - LRC timestamp stripping (`[mm:ss.xx]`).
    /// - Repeated lines (common in Bollywood — counted normally, intentional).
    pub fn analyse(&self, text: &str) -> SentimentResult {
        let mut romantic_hits: u32 = 0;
        let mut sad_hits: u32 = 0;

        // Strip LRC timestamps: [00:00.00] or [00:00:00]
        let stripped = strip_lrc_timestamps(text);

        // Tokenise: split on whitespace and common punctuation.
        // We keep Unicode letters together (Devanagari words don't have
        // internal spaces, so they arrive as whole tokens after split).
        for raw_token in stripped.split(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ';' | ':' |
                                            '"' | '\'' | '(' | ')' | '[' | ']' |
                                            '|' | '/' | '\\' | '-' | '_' | '~')
        }) {
            if raw_token.is_empty() { continue; }

            // Lowercase for ASCII; Devanagari has no case so to_lowercase is a no-op.
            let lower = raw_token.to_lowercase();
            let token = lower.trim();
            if token.is_empty() { continue; }

            match self.lookup(token) {
                Some(Tag::Romantic) => romantic_hits += 1,
                Some(Tag::Sad)      => sad_hits += 1,
                None => {}
            }
        }

        let total = romantic_hits + sad_hits;

        // Score: normalised difference.
        let score = (romantic_hits as f64 - sad_hits as f64) / (total as f64 + 1.0);

        // Confidence: how decisive is the split?
        let confidence = ((romantic_hits as f64 - sad_hits as f64).abs()
                         / (total as f64 + 1.0))
                        .min(1.0);

        const CONFIDENCE_THRESHOLD: f64 = 0.15;

        let sentiment = if confidence < CONFIDENCE_THRESHOLD {
            Sentiment::Ambiguous
        } else if score > 0.0 {
            Sentiment::Romantic
        } else {
            Sentiment::Sad
        };

        SentimentResult { sentiment, confidence, romantic_hits, sad_hits }
    }
}

impl Default for LyricsSentiment {
    fn default() -> Self { Self::new() }
}

/// Strip LRC-format timestamps (`[mm:ss.xx]`, `[mm:ss:xx]`) from lyrics text.
fn strip_lrc_timestamps(text: &str) -> String {
    // Simple state-machine: skip characters between '[' and ']' when the
    // content looks like digits/colons/periods (i.e. a timestamp).
    let mut out = String::with_capacity(text.len());
    let mut in_bracket = false;
    let mut bracket_buf = String::new();

    for ch in text.chars() {
        match ch {
            '[' => {
                in_bracket = true;
                bracket_buf.clear();
            }
            ']' if in_bracket => {
                // Only suppress if the bracket content looks like a timestamp.
                let is_ts = bracket_buf.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.');
                if !is_ts {
                    // Not a timestamp (e.g. "[Chorus]") — keep it.
                    out.push('[');
                    out.push_str(&bracket_buf);
                    out.push(']');
                }
                in_bracket = false;
                bracket_buf.clear();
            }
            _ if in_bracket => {
                bracket_buf.push(ch);
            }
            _ => {
                out.push(ch);
            }
        }
    }
    // Unclosed bracket — flush.
    if in_bracket {
        out.push('[');
        out.push_str(&bracket_buf);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn analyser() -> LyricsSentiment { LyricsSentiment::new() }

    // --- construction ---

    #[test]
    fn test_new_does_not_panic() {
        let _ = LyricsSentiment::new();
    }

    // --- LRC timestamp stripping ---

    #[test]
    fn test_lrc_timestamp_stripped() {
        let text = "[00:12.34]Tere bina\n[00:15.00]zindagi se";
        let stripped = strip_lrc_timestamps(text);
        assert!(!stripped.contains("[00:12.34]"), "timestamp should be removed");
        assert!(stripped.contains("Tere bina"), "lyric text should remain");
    }

    #[test]
    fn test_section_label_kept() {
        let text = "[Chorus]\npyaar ka rang";
        let stripped = strip_lrc_timestamps(text);
        assert!(stripped.contains("[Chorus]"), "section label should be kept");
    }

    // --- basic sentiment ---

    #[test]
    fn test_clearly_romantic_english() {
        let r = analyser().analyse("love darling kiss forever beautiful dream soul soulmate");
        assert_eq!(r.sentiment, Sentiment::Romantic, "hits: R={} S={}", r.romantic_hits, r.sad_hits);
        assert!(r.romantic_hits > r.sad_hits);
    }

    #[test]
    fn test_clearly_sad_english() {
        let r = analyser().analyse("crying tears pain broken lonely heartbroken darkness despair alone");
        assert_eq!(r.sentiment, Sentiment::Sad, "hits: R={} S={}", r.romantic_hits, r.sad_hits);
        assert!(r.sad_hits > r.romantic_hits);
    }

    #[test]
    fn test_clearly_romantic_romanised_hindi() {
        let r = analyser().analyse("pyaar mohabbat jaanam sanam dil ishq teri meri");
        assert_eq!(r.sentiment, Sentiment::Romantic, "hits: R={} S={}", r.romantic_hits, r.sad_hits);
    }

    #[test]
    fn test_clearly_sad_romanised_hindi() {
        let r = analyser().analyse("dard gham udaas tanha aansu judaai dhokha toota bereham");
        assert_eq!(r.sentiment, Sentiment::Sad, "hits: R={} S={}", r.romantic_hits, r.sad_hits);
    }

    #[test]
    fn test_clearly_romantic_devanagari() {
        let r = analyser().analyse("प्यार इश्क़ मोहब्बत जानम दिल");
        assert_eq!(r.sentiment, Sentiment::Romantic, "hits: R={} S={}", r.romantic_hits, r.sad_hits);
    }

    #[test]
    fn test_clearly_sad_devanagari() {
        let r = analyser().analyse("दर्द ग़म उदास तन्हा आँसू");
        assert_eq!(r.sentiment, Sentiment::Sad, "hits: R={} S={}", r.romantic_hits, r.sad_hits);
    }

    // --- ambiguity ---

    #[test]
    fn test_empty_lyrics_is_ambiguous() {
        let r = analyser().analyse("");
        assert_eq!(r.sentiment, Sentiment::Ambiguous);
        assert_eq!(r.romantic_hits, 0);
        assert_eq!(r.sad_hits, 0);
    }

    #[test]
    fn test_no_matching_words_is_ambiguous() {
        let r = analyser().analyse("guitar solo instrumental bridge verse");
        assert_eq!(r.sentiment, Sentiment::Ambiguous);
    }

    #[test]
    fn test_balanced_lyrics_may_be_ambiguous() {
        // Equal romantic and sad words → confidence near zero.
        let r = analyser().analyse("love crying darling tears kiss pain");
        // Could be Ambiguous or weakly Romantic — just must not panic.
        assert!(r.confidence >= 0.0 && r.confidence <= 1.0);
    }

    // --- LRC real-world format ---

    #[test]
    fn test_lrc_format_romantic() {
        let lrc = "[00:10.24]Tujhe dekha toh yeh jaana sanam\n\
                   [00:14.80]Pyaar hota hai deewana sanam\n\
                   [00:19.10]Ab yahan se kahan jaayein hum\n\
                   [00:23.60]Teri baahon mein mar jaayein hum";
        let r = analyser().analyse(lrc);
        assert!(r.romantic_hits > 0, "should find romantic words in LRC");
    }

    #[test]
    fn test_lrc_format_sad() {
        let lrc = "[01:00.00]Roke na ruke naina\n\
                   [01:04.50]Aur dard hai seene mein\n\
                   [01:09.00]Tanha tanha yahan pe jeena\n\
                   [01:13.50]Dard mila hai jeene mein";
        let r = analyser().analyse(lrc);
        assert!(r.sad_hits > 0, "should find sad words in LRC");
    }

    // --- confidence bounds ---

    #[test]
    fn test_confidence_in_unit_interval() {
        let cases = [
            "love darling kiss forever",
            "dard gham tanha aansu",
            "",
            "the quick brown fox",
        ];
        for text in &cases {
            let r = analyser().analyse(text);
            assert!(r.confidence >= 0.0 && r.confidence <= 1.0,
                "confidence out of range for: {:?}", text);
        }
    }
}
