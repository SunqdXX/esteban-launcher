use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Number(u64),
    Any,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    raw: String,
    semantic: Option<Semantic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Semantic {
    parts: Vec<Part>,
    pre: Option<String>,
}

impl Version {
    pub fn parse(raw: &str) -> Self {
        Self {
            raw: raw.to_string(),
            semantic: semantic(raw, false),
        }
    }

    pub fn short(&self) -> &str {
        self.raw.split('+').next().unwrap_or(&self.raw)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

fn semantic(raw: &str, allow_any: bool) -> Option<Semantic> {
    let without_build = raw.split_once('+').map_or(raw, |(core, _)| core);
    let (core, pre) = match without_build.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (without_build, None),
    };
    if let Some(pre) = pre
        && !pre.is_empty()
        && !pre
            .split('.')
            .all(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        return None;
    }
    if core.is_empty() || core.ends_with('.') {
        return None;
    }
    let mut parts = Vec::new();
    for piece in core.split('.') {
        let part = if allow_any && matches!(piece, "x" | "X" | "*") {
            Part::Any
        } else if !piece.is_empty() && piece.chars().all(|c| c.is_ascii_digit()) {
            Part::Number(piece.parse().ok()?)
        } else {
            return None;
        };
        if parts.last() == Some(&Part::Any) && part != Part::Any {
            return None;
        }
        parts.push(part);
    }
    let has_any = parts.contains(&Part::Any);
    if has_any && pre.is_some() {
        return None;
    }
    Some(Semantic {
        parts,
        pre: pre.map(str::to_string),
    })
}

impl Semantic {
    fn part(&self, index: usize) -> Part {
        match self.parts.get(index) {
            Some(part) => *part,
            None if self.parts.last() == Some(&Part::Any) => Part::Any,
            None => Part::Number(0),
        }
    }

    fn has_any(&self) -> bool {
        self.parts.contains(&Part::Any)
    }

    fn compare(&self, other: &Self) -> Ordering {
        for index in 0..self.parts.len().max(other.parts.len()) {
            if let (Part::Number(a), Part::Number(b)) = (self.part(index), other.part(index)) {
                match a.cmp(&b) {
                    Ordering::Equal => {}
                    unequal => return unequal,
                }
            }
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (Some(_), None) if other.has_any() => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) if self.has_any() => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => compare_pre(a, b),
        }
    }
}

fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut left = a.split('.').filter(|t| !t.is_empty());
    let mut right = b.split('.').filter(|t| !t.is_empty());
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    AtLeast,
    AtMost,
    Above,
    Below,
    Exactly,
    SameMinor,
    SameMajor,
}

const OPERATORS: &[(&str, Op)] = &[
    (">=", Op::AtLeast),
    ("<=", Op::AtMost),
    (">", Op::Above),
    ("<", Op::Below),
    ("=", Op::Exactly),
    ("~", Op::SameMinor),
    ("^", Op::SameMajor),
];

#[derive(Clone, Debug)]
struct Term {
    op: Op,
    raw: String,
    semantic: Option<Semantic>,
}

impl Term {
    fn parse(text: &str) -> Option<Self> {
        let (mut op, rest) = OPERATORS
            .iter()
            .find_map(|(symbol, op)| text.strip_prefix(symbol).map(|rest| (*op, rest)))
            .unwrap_or((Op::Exactly, text));
        match semantic(rest, true) {
            Some(mut sem) if sem.has_any() => {
                if op != Op::Exactly {
                    return None;
                }
                let fixed: Vec<Part> = sem
                    .parts
                    .iter()
                    .copied()
                    .take_while(|p| *p != Part::Any)
                    .collect();
                op = if sem.parts.len() <= 2 {
                    Op::SameMajor
                } else {
                    Op::SameMinor
                };
                sem.parts = fixed;
                sem.pre = Some(String::new());
                Some(Self {
                    op,
                    raw: rest.to_string(),
                    semantic: Some(sem),
                })
            }
            Some(sem) => Some(Self {
                op,
                raw: rest.to_string(),
                semantic: Some(sem),
            }),
            None if matches!(op, Op::Above | Op::Below) => None,
            None => Some(Self {
                op: Op::Exactly,
                raw: rest.to_string(),
                semantic: None,
            }),
        }
    }

    fn matches(&self, version: &Version) -> bool {
        let (Some(have), Some(want)) = (&version.semantic, &self.semantic) else {
            return version.raw == self.raw;
        };
        let order = have.compare(want);
        let same = |index| match (have.part(index), want.part(index)) {
            (Part::Number(a), Part::Number(b)) => a == b,
            _ => true,
        };
        match self.op {
            Op::AtLeast => order != Ordering::Less,
            Op::AtMost => order != Ordering::Greater,
            Op::Above => order == Ordering::Greater,
            Op::Below => order == Ordering::Less,
            Op::Exactly => order == Ordering::Equal,
            Op::SameMinor => order != Ordering::Less && same(0) && same(1),
            Op::SameMajor => order != Ordering::Less && same(0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Requirement {
    text: String,
    alternatives: Vec<Vec<Term>>,
}

impl Requirement {
    pub fn parse(alternatives: &[String]) -> Option<Self> {
        let mut parsed = Vec::new();
        for alternative in alternatives {
            let mut terms = Vec::new();
            for piece in alternative.split(' ').map(str::trim) {
                if piece.is_empty() || piece == "*" {
                    continue;
                }
                terms.push(Term::parse(piece)?);
            }
            parsed.push(terms);
        }
        Some(Self {
            text: alternatives.join(" or "),
            alternatives: parsed,
        })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.alternatives.is_empty()
            || self
                .alternatives
                .iter()
                .any(|terms| terms.iter().all(|t| t.matches(version)))
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str) -> Requirement {
        Requirement::parse(&[text.to_string()]).unwrap()
    }

    fn ok(text: &str, version: &str) -> bool {
        req(text).matches(&Version::parse(version))
    }

    #[test]
    fn x_ranges_match_the_same_minor_or_major() {
        assert!(ok("0.9.x", "0.9.2+mc26.3"));
        assert!(ok("0.9.x", "0.9.0"));
        assert!(!ok("0.9.x", "0.8.9+mc26.1.1"));
        assert!(!ok("0.9.x", "0.10.0"));
        assert!(ok("1.21.x", "1.21.4"));
        assert!(!ok("1.21.x", "1.22"));
        assert!(ok("1.x", "1.99.1"));
        assert!(!ok("1.x", "2.0.0"));
    }

    #[test]
    fn plain_operators_and_and_lists() {
        assert!(ok(">=0.16.0", "0.19.5"));
        assert!(!ok(">=0.16.9", "0.16.8"));
        assert!(ok(">=1.21.4- <1.21.5-", "1.21.4"));
        assert!(ok(">=1.21.4- <1.21.5-", "1.21.4-pre1"));
        assert!(!ok(">=1.21.4- <1.21.5-", "1.21.5"));
        assert!(ok(">=1.21.4 <1.22", "1.21.4"));
        assert!(ok("<=0.3", "0.3"));
        assert!(!ok("<1.8.7", "1.8.8+mc1.21.4"));
        assert!(ok("~1.2.3", "1.2.9"));
        assert!(!ok("~1.2.3", "1.3.0"));
        assert!(ok("^1.2.3", "1.9.0"));
        assert!(!ok("^1.2.3", "2.0.0"));
        assert!(ok("*", "anything"));
        assert!(ok(">=21", "21"));
    }

    #[test]
    fn exact_versions_ignore_build_metadata() {
        assert!(ok("1.21.4", "1.21.4"));
        assert!(ok("=1.3.0", "1.3.0+1.21.4"));
        assert!(!ok("1.21.4", "1.21.5"));
        assert!(ok("26.1", "26.1.0"));
    }

    #[test]
    fn alternatives_are_an_or() {
        let r = Requirement::parse(&["0.6.x".to_string(), "0.9.x".to_string()]).unwrap();
        assert!(r.matches(&Version::parse("0.9.2")));
        assert!(r.matches(&Version::parse("0.6.13")));
        assert!(!r.matches(&Version::parse("0.7.0")));
        assert_eq!(r.text(), "0.6.x or 0.9.x");
        assert!(
            Requirement::parse(&[])
                .unwrap()
                .matches(&Version::parse("1"))
        );
    }

    #[test]
    fn prereleases_sort_below_releases() {
        let a = semantic("1.0.0-alpha", false).unwrap();
        let b = semantic("1.0.0-alpha.1", false).unwrap();
        let c = semantic("1.0.0-beta", false).unwrap();
        let d = semantic("1.0.0", false).unwrap();
        assert_eq!(a.compare(&b), Ordering::Less);
        assert_eq!(b.compare(&c), Ordering::Less);
        assert_eq!(c.compare(&d), Ordering::Less);
        assert_eq!(
            semantic("1.0.0-2", false)
                .unwrap()
                .compare(&semantic("1.0.0-10", false).unwrap()),
            Ordering::Less
        );
    }

    #[test]
    fn text_versions_only_match_exactly() {
        assert!(ok("mc1.21.4-0.6.13", "mc1.21.4-0.6.13"));
        assert!(!ok("mc1.21.4-0.6.13", "mc1.21.4-0.6.14"));
        assert!(Requirement::parse(&[">nonsense".to_string()]).is_none());
        assert!(Requirement::parse(&[">=1.x".to_string()]).is_none());
    }

    #[test]
    fn short_drops_build_metadata() {
        assert_eq!(Version::parse("0.8.9+mc26.1.1").short(), "0.8.9");
        assert_eq!(Version::parse("7.1.3").short(), "7.1.3");
    }
}
