const MAX_PATTERN_BYTES: usize = 64 * 1024;

enum GlobAtom {
    Literal(char),
    Any,
    Star,
}

pub fn validate_glob(pattern: &str) -> Result<(), String> {
    parse_glob(pattern).map(|_| ())
}

pub fn glob_matches(pattern: &str, haystack: &str, case_insensitive: bool) -> bool {
    parse_glob(pattern).is_ok_and(|atoms| match_glob(&atoms, haystack, case_insensitive))
}

pub fn validate_regex(pattern: &str, case_insensitive: bool) -> Result<(), String> {
    compile_regex(pattern, case_insensitive).map(|_| ())
}

pub fn regex_matches(pattern: &str, haystack: &str, case_insensitive: bool) -> bool {
    compile_regex(pattern, case_insensitive).is_ok_and(|compiled| compiled.is_match(haystack))
}

fn bounded_pattern(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("must not be empty".to_owned());
    }
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err("exceeds the 65536-byte limit".to_owned());
    }
    Ok(())
}

fn parse_glob(pattern: &str) -> Result<Vec<GlobAtom>, String> {
    bounded_pattern(pattern)?;
    let mut atoms = Vec::new();
    let mut chars = pattern.chars();
    while let Some(next) = chars.next() {
        match next {
            '\\' => {
                let escaped = chars
                    .next()
                    .ok_or_else(|| "trailing backslash".to_owned())?;
                atoms.push(GlobAtom::Literal(escaped));
            }
            '*' => atoms.push(GlobAtom::Star),
            '?' => atoms.push(GlobAtom::Any),
            literal => atoms.push(GlobAtom::Literal(literal)),
        }
    }
    Ok(atoms)
}

fn match_glob(atoms: &[GlobAtom], haystack: &str, case_insensitive: bool) -> bool {
    let text: Vec<char> = haystack.chars().collect();
    let mut pattern_index = 0;
    let mut text_index = 0;
    let mut star = None;
    while text_index < text.len() {
        if pattern_index < atoms.len() {
            match atoms[pattern_index] {
                GlobAtom::Any => {
                    pattern_index += 1;
                    text_index += 1;
                    continue;
                }
                GlobAtom::Literal(expected)
                    if chars_equal(expected, text[text_index], case_insensitive) =>
                {
                    pattern_index += 1;
                    text_index += 1;
                    continue;
                }
                GlobAtom::Star => {
                    star = Some((pattern_index + 1, text_index));
                    pattern_index += 1;
                    continue;
                }
                GlobAtom::Literal(_) => {}
            }
        }
        if let Some((resume_pattern, resume_text)) = star {
            text_index = resume_text + 1;
            star = Some((resume_pattern, text_index));
            pattern_index = resume_pattern;
        } else {
            return false;
        }
    }
    while pattern_index < atoms.len() && matches!(atoms[pattern_index], GlobAtom::Star) {
        pattern_index += 1;
    }
    pattern_index == atoms.len()
}

fn chars_equal(expected: char, actual: char, case_insensitive: bool) -> bool {
    if case_insensitive {
        expected.to_lowercase().eq(actual.to_lowercase())
    } else {
        expected == actual
    }
}

fn compile_regex(pattern: &str, case_insensitive: bool) -> Result<regex::Regex, String> {
    bounded_pattern(pattern)?;
    let wrapped = format!("\\A(?:{pattern})\\z");
    regex::RegexBuilder::new(&wrapped)
        .case_insensitive(case_insensitive)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_case_insensitive_glob_matches_whole_url() {
        assert!(glob_matches(
            "https://example.com/CAFÉ*",
            "https://example.com/Café/Page",
            true,
        ));
        assert!(!glob_matches(
            "https://example.com/CAFÉ*",
            "https://example.com/Café/Page",
            false,
        ));
    }
}
