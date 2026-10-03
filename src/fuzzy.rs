//! Fuzzy matching of short strings such as note titles: the letters of the
//! query must appear in order, and matches at word starts, in runs and near
//! the beginning rank higher.

/// A successful match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    /// Character indices of `candidate` that matched the query, ascending.
    pub positions: Vec<usize>,
}

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 18;
const WORD_START: i32 = 24;
const FIRST_CHAR: i32 = 16;
const GAP: i32 = 2;
const EXACT: i32 = 400;
const PREFIX: i32 = 120;

/// Matches `query` against `candidate`, ignoring case and spaces in the query.
/// An empty query matches everything with score 0.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<Match> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    if query.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let chars: Vec<char> = candidate.chars().collect();
    let folded: Vec<char> = chars.iter().copied().map(fold).collect();

    // Try every place the first letter occurs and keep the best greedy match
    // from there: cheap, and good enough for titles.
    let mut best: Option<Match> = None;
    for start in (0..folded.len()).filter(|&i| folded[i] == query[0]) {
        let Some(positions) = greedy(&folded, &query, start) else {
            break;
        };
        let score = score(&chars, &positions);
        if best.as_ref().is_none_or(|b| score > b.score) {
            best = Some(Match { score, positions });
        }
    }
    let mut best = best?;

    let compact: String = folded.iter().filter(|c| !c.is_whitespace()).collect();
    let query: String = query.into_iter().collect();
    if compact == query {
        best.score += EXACT;
    } else if compact.starts_with(&query) {
        best.score += PREFIX;
    }
    // Among equal matches, shorter candidates first.
    best.score -= chars.len() as i32 / 4;
    Some(best)
}

/// Positions of `query` in `text`, taking each letter at its first occurrence
/// after the previous one, starting at `start`.
fn greedy(text: &[char], query: &[char], start: usize) -> Option<Vec<usize>> {
    let mut positions = Vec::with_capacity(query.len());
    let mut i = start;
    for &q in query {
        while i < text.len() && text[i] != q {
            i += 1;
        }
        if i == text.len() {
            return None;
        }
        positions.push(i);
        i += 1;
    }
    Some(positions)
}

fn score(chars: &[char], positions: &[usize]) -> i32 {
    let mut score = 0;
    for (n, &p) in positions.iter().enumerate() {
        score += MATCH;
        if is_word_start(chars, p) {
            score += WORD_START;
        }
        if p == 0 {
            score += FIRST_CHAR;
        }
        if n > 0 {
            let prev = positions[n - 1];
            if p == prev + 1 {
                score += CONSECUTIVE;
            } else {
                score -= GAP * (p - prev - 1).min(8) as i32;
            }
        } else {
            score -= (p as i32).min(12);
        }
    }
    score
}

fn is_word_start(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let (prev, cur) = (chars[i - 1], chars[i]);
    !prev.is_alphanumeric() || (prev.is_lowercase() && cur.is_uppercase())
}

/// Case folding that keeps one character per character, so indices stay valid.
pub fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The indices of `candidates` that match `query`, best first, at most `limit`.
pub fn rank<'a>(
    query: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    limit: usize,
) -> Vec<(usize, Match)> {
    let mut hits: Vec<(usize, Match, &str)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| fuzzy_match(query, c).map(|m| (i, m, c)))
        .collect();
    hits.sort_by(|a, b| {
        b.1.score
            .cmp(&a.1.score)
            .then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase()))
    });
    hits.truncate(limit);
    hits.into_iter().map(|(i, m, _)| (i, m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(query: &str, candidates: &[&str]) -> Vec<String> {
        rank(query, candidates.iter().copied(), 10)
            .into_iter()
            .map(|(i, _)| candidates[i].to_owned())
            .collect()
    }

    #[test]
    fn letters_must_appear_in_order() {
        assert!(fuzzy_match("pln", "Plan").is_some());
        assert!(fuzzy_match("npl", "Plan").is_none());
        assert!(fuzzy_match("", "Plan").is_some());
        assert_eq!(fuzzy_match("PL", "a plan").unwrap().positions, [2, 3]);
    }

    #[test]
    fn exact_prefix_and_word_starts_rank_first() {
        assert_eq!(
            order(
                "plan",
                &["Explanation", "Plan for May", "plan", "Peeling lanterns"]
            ),
            ["plan", "Plan for May", "Explanation", "Peeling lanterns"]
        );
        assert_eq!(
            order("wr", &["Grocery Weekly Run", "fwr", "Writing"]),
            ["Writing", "Grocery Weekly Run", "fwr"]
        );
        assert_eq!(order("mn", &["Meeting notes", "lemon"])[0], "Meeting notes");
    }

    #[test]
    fn prefers_the_best_occurrence_of_the_first_letter() {
        // Greedy from the first "n" would match n..o..t spread out; "Notes" is a run.
        let m = fuzzy_match("not", "nightly Notes").unwrap();
        assert_eq!(m.positions, [8, 9, 10]);
    }

    #[test]
    fn handles_accents_and_spaces() {
        assert!(fuzzy_match("ÑAN dú", "ñandú").is_some());
        assert!(fuzzy_match("week 1", "Week 1").unwrap().score > 400);
    }
}
