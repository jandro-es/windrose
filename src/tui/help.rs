//! The glossary behind the `?` key.
//!
//! Windrose's plain-language rule says no unexplained jargon reaches the user.
//! Some words are unavoidable — they are printed on the screens themselves, in
//! model names and in other tools' output — so this is where they get
//! explained, once, in words that assume no background at all.

/// Terms a reader may meet in Windrose or in the tools it finds, each with a
/// beginner's explanation.
pub fn glossary() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "model",
            "The AI itself — a large file that has been trained to answer questions. Bigger \
             models generally give better answers and run more slowly.",
        ),
        (
            "quantisation",
            "Compressing a model so it takes less memory. \"Q4\" means about a quarter of the \
             original size. It is how models are normally run on a Mac, and costs a little \
             accuracy.",
        ),
        (
            "token",
            "A chunk of text a model reads and writes, a little shorter than a word. Speeds are \
             given in tokens per second; around 30 is comfortable reading pace.",
        ),
        (
            "context window",
            "How much of the conversation a model can keep in mind at once. When a long chat \
             runs past it, the earliest part is forgotten.",
        ),
        (
            "on-device",
            "Running on your own Mac rather than on a company's servers. Nothing you type \
             leaves the machine, and it works without an internet connection.",
        ),
        (
            "API key",
            "A password that lets a program use a paid online service. Windrose only ever \
             checks whether one exists — it never reads, stores or displays the key itself.",
        ),
        (
            "runtime",
            "The program that actually runs a model, such as Ollama or LM Studio. The model is \
             the file; the runtime is what opens it.",
        ),
        (
            "parameters",
            "A rough measure of a model's size, counted in billions and written like \"8B\". \
             More parameters usually means better answers and more memory needed.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The terms the spec requires, at minimum.
    #[test]
    fn the_required_terms_are_all_explained() {
        let terms: Vec<&str> = glossary().iter().map(|(term, _)| *term).collect();

        for required in [
            "model",
            "quantisation",
            "token",
            "context window",
            "on-device",
            "API key",
        ] {
            assert!(terms.contains(&required), "glossary is missing: {required}");
        }
    }

    /// An explanation that leans on other jargon has not explained anything.
    #[test]
    fn explanations_are_written_for_a_beginner() {
        for (term, explanation) in glossary() {
            assert!(
                explanation.len() > 60,
                "{term}: too short to explain anything"
            );
            assert!(
                explanation.ends_with('.'),
                "{term}: should read as full sentences"
            );
            assert!(
                !explanation.contains("LLM") && !explanation.contains("inference"),
                "{term}: explanation uses jargon of its own"
            );
        }
    }

    /// The secrets rule is a promise to the user, so the glossary states it
    /// where the user is most likely to wonder.
    #[test]
    fn the_api_key_entry_states_what_windrose_does_not_do() {
        let (_, explanation) = glossary()
            .into_iter()
            .find(|(term, _)| *term == "API key")
            .expect("API key is a required term");

        assert!(explanation.contains("never"));
    }

    #[test]
    fn terms_are_not_duplicated() {
        let mut terms: Vec<&str> = glossary().iter().map(|(t, _)| *t).collect();
        let count = terms.len();
        terms.sort_unstable();
        terms.dedup();
        assert_eq!(terms.len(), count, "duplicate glossary term");
    }
}
