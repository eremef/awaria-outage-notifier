use regex::Regex;

fn main() {
    let word = "Wrocław";
    // ASCII-only \b
    let pattern_ascii = format!(r"(?i)\b{}\b", regex::escape(word));
    let re_ascii = Regex::new(&pattern_ascii).unwrap();
    let text = "Wrocław, ul. Wieniawskiego";
    println!("ASCII \\b match '{}' in '{}': {}", word, text, re_ascii.is_match(text));

    // Unicode boundary attempt (though \b is ASCII-only in many versions)
    let pattern_u = format!(r"(?ui)\b{}\b", regex::escape(word));
    let re_u = Regex::new(&pattern_u).unwrap();
    println!("Unicode \\b match '{}' in '{}': {}", word, text, re_u.is_match(text));

    // Manual boundary matching non-letters
    let pattern_manual = format!(r"(?i)(?:^|[^p{{L}}]){}(?:[^p{{L}}]|$)", regex::escape(word));
    // Wait, Rust regex uses \p{L} not p{L} and needs double escape or raw string
    let pattern_manual_correct = format!(r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)", regex::escape(word));
    let re_manual = Regex::new(&pattern_manual_correct).unwrap();
    println!("Manual boundary match '{}' in '{}': {}", word, text, re_manual.is_match(text));
}
