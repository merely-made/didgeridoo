//! The Lua 5.4 pattern matcher, over byte strings.
//!
//! The matcher never recurses on the native stack. Backtracking points live in a `Vec` of
//! [`Frame`]s whose length is capped by [`MAX_PATTERN_DEPTH`]. Every step is counted, and a call
//! that passes [`MAX_MATCH_STEPS`] fails with an error. The library glue charges the same count to
//! the running script's fuel.

/// Most captures one pattern may hold.
pub const MAX_CAPTURES: usize = 32;

/// Most pending backtracking points (and capture undo records) at once. Lua's own matcher
/// recurses once per such point and stops at 200, so scripts that run there run here.
pub const MAX_PATTERN_DEPTH: usize = 200;

/// Most matcher steps one library call may take before it fails with an error. A callback cannot
/// be interrupted midway by fuel, so this is what bounds a single call.
pub const MAX_MATCH_STEPS: u64 = 32_000_000;

const CAP_UNFINISHED: usize = usize::MAX;
const CAP_POSITION: usize = usize::MAX - 1;

/// A pattern failure, carrying the message raised to the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError(pub String);

impl PatternError {
    fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

type Res<T> = Result<T, PatternError>;

#[derive(Clone, Copy)]
struct Capture {
    start: usize,
    len: usize,
}

/// A capture as the script sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureValue {
    /// Subject bytes `start..end`.
    Span(usize, usize),
    /// A `()` capture: the 1-based position.
    Position(usize),
}

enum Frame {
    /// `*` or `+`: the rest of the pattern at `p` is tried at `hi`, then `hi - 1`, down to `lo`.
    Greedy { p: usize, lo: usize, hi: usize },
    /// `-`: the item at `item..ep` may take one more byte at `s` and retry the rest.
    Lazy { item: usize, ep: usize, s: usize },
    /// `?` took its byte; the alternative is the rest at `p` from `s`.
    Skip { p: usize, s: usize },
    /// Undo a capture opened here.
    CapStart,
    /// Undo the closing of capture `.0`.
    CapEnd(usize),
}

enum Step {
    Next(usize, usize),
    Done,
    Fail,
}

pub struct Matcher<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    caps: [Capture; MAX_CAPTURES],
    level: usize,
    frames: Vec<Frame>,
    steps: u64,
}

impl<'a> Matcher<'a> {
    pub fn new(src: &'a [u8], pat: &'a [u8]) -> Self {
        Self {
            src,
            pat,
            caps: [Capture { start: 0, len: 0 }; MAX_CAPTURES],
            level: 0,
            frames: Vec::new(),
            steps: 0,
        }
    }

    /// Steps taken so far across all calls on this matcher.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Returns the steps taken so far and starts counting again from zero.
    pub fn take_steps(&mut self) -> u64 {
        std::mem::take(&mut self.steps)
    }

    /// Adds work done outside the matcher proper (such as a plain search) to the step count.
    pub fn tick(&mut self, n: u64) -> Res<()> {
        self.steps = self.steps.saturating_add(n);
        if self.steps > MAX_MATCH_STEPS {
            Err(PatternError::new("pattern match step limit exceeded"))
        } else {
            Ok(())
        }
    }

    /// Number of captures the last successful match produced.
    pub fn level(&self) -> usize {
        self.level
    }

    /// The `i`th capture (0-based) of the last match `s..e`. With no explicit captures, index 0
    /// is the whole match.
    pub fn capture(&self, i: usize, s: usize, e: usize) -> Res<CaptureValue> {
        if i >= self.level {
            return if i == 0 {
                Ok(CaptureValue::Span(s, e))
            } else {
                Err(PatternError::new(format!(
                    "invalid capture index %{}",
                    i + 1
                )))
            };
        }
        let cap = self.caps[i];
        match cap.len {
            CAP_UNFINISHED => Err(PatternError::new("unfinished capture")),
            CAP_POSITION => Ok(CaptureValue::Position(cap.start + 1)),
            len => Ok(CaptureValue::Span(cap.start, cap.start + len)),
        }
    }

    /// Finds the first match starting at or after `init`. With `anchored`, only `init` is tried.
    /// `p0` is where the pattern body starts (1 when it has a leading `^`).
    pub fn search(
        &mut self,
        init: usize,
        p0: usize,
        anchored: bool,
    ) -> Res<Option<(usize, usize)>> {
        let mut s = init;
        loop {
            if let Some(e) = self.match_at(s, p0)? {
                return Ok(Some((s, e)));
            }
            s += 1;
            if anchored || s > self.src.len() {
                return Ok(None);
            }
        }
    }

    /// Tries to match the pattern from `p0` at exactly `s0`, returning the end of the match.
    pub fn match_at(&mut self, s0: usize, p0: usize) -> Res<Option<usize>> {
        self.level = 0;
        self.frames.clear();
        let (mut p, mut s) = (p0, s0);
        loop {
            self.tick(1)?;
            let next = match self.step(p, s)? {
                Step::Next(np, ns) => Some((np, ns)),
                Step::Done => return Ok(Some(s)),
                Step::Fail => self.backtrack()?,
            };
            match next {
                Some((np, ns)) => (p, s) = (np, ns),
                None => return Ok(None),
            }
        }
    }

    fn push(&mut self, frame: Frame) -> Res<()> {
        if self.frames.len() >= MAX_PATTERN_DEPTH {
            return Err(PatternError::new("pattern too complex"));
        }
        self.frames.push(frame);
        Ok(())
    }

    fn backtrack(&mut self) -> Res<Option<(usize, usize)>> {
        loop {
            self.tick(1)?;
            match self.frames.pop() {
                None => return Ok(None),
                Some(Frame::CapStart) => self.level -= 1,
                Some(Frame::CapEnd(l)) => self.caps[l].len = CAP_UNFINISHED,
                Some(Frame::Skip { p, s }) => return Ok(Some((p, s))),
                Some(Frame::Greedy { p, lo, hi }) => {
                    if hi > lo {
                        let hi = hi - 1;
                        if hi > lo {
                            self.frames.push(Frame::Greedy { p, lo, hi });
                        }
                        return Ok(Some((p, hi)));
                    }
                }
                Some(Frame::Lazy { item, ep, s }) => {
                    if self.single_match(s, item, ep) {
                        self.frames.push(Frame::Lazy { item, ep, s: s + 1 });
                        return Ok(Some((ep + 1, s + 1)));
                    }
                }
            }
        }
    }

    fn step(&mut self, p: usize, s: usize) -> Res<Step> {
        let pat = self.pat;
        if p >= pat.len() {
            return Ok(Step::Done);
        }
        match pat[p] {
            b'(' => {
                if pat.get(p + 1) == Some(&b')') {
                    self.open_capture(s, CAP_POSITION)?;
                    Ok(Step::Next(p + 2, s))
                } else {
                    self.open_capture(s, CAP_UNFINISHED)?;
                    Ok(Step::Next(p + 1, s))
                }
            }
            b')' => {
                let l = (0..self.level)
                    .rev()
                    .find(|&l| self.caps[l].len == CAP_UNFINISHED)
                    .ok_or_else(|| PatternError::new("invalid pattern capture"))?;
                self.caps[l].len = s - self.caps[l].start;
                self.push(Frame::CapEnd(l))?;
                Ok(Step::Next(p + 1, s))
            }
            b'$' if p + 1 == pat.len() => Ok(if s == self.src.len() {
                Step::Done
            } else {
                Step::Fail
            }),
            b'%' => match pat.get(p + 1) {
                Some(b'b') => self.balance(p, s),
                Some(b'f') => self.frontier(p, s),
                Some(d) if d.is_ascii_digit() => self.back_reference(p, s, *d),
                _ => self.item(p, s),
            },
            _ => self.item(p, s),
        }
    }

    fn open_capture(&mut self, s: usize, len: usize) -> Res<()> {
        if self.level >= MAX_CAPTURES {
            return Err(PatternError::new("too many captures"));
        }
        self.push(Frame::CapStart)?;
        self.caps[self.level] = Capture { start: s, len };
        self.level += 1;
        Ok(())
    }

    fn balance(&mut self, p: usize, s: usize) -> Res<Step> {
        if p + 3 >= self.pat.len() {
            return Err(PatternError::new(
                "malformed pattern (missing arguments to '%b')",
            ));
        }
        let (open, close) = (self.pat[p + 2], self.pat[p + 3]);
        if self.src.get(s) != Some(&open) {
            return Ok(Step::Fail);
        }
        let mut depth = 1usize;
        let mut i = s + 1;
        while i < self.src.len() {
            let c = self.src[i];
            i += 1;
            if c == close {
                depth -= 1;
                if depth == 0 {
                    self.tick((i - s) as u64)?;
                    return Ok(Step::Next(p + 4, i));
                }
            } else if c == open {
                depth += 1;
            }
        }
        self.tick((i - s) as u64)?;
        Ok(Step::Fail)
    }

    fn frontier(&mut self, p: usize, s: usize) -> Res<Step> {
        if self.pat.get(p + 2) != Some(&b'[') {
            return Err(PatternError::new("missing '[' after '%f' in pattern"));
        }
        let ep = self.class_end(p + 2)?;
        let prev = if s == 0 { 0 } else { self.src[s - 1] };
        let cur = self.src.get(s).copied().unwrap_or(0);
        Ok(
            if !self.in_set(prev, p + 2, ep - 1) && self.in_set(cur, p + 2, ep - 1) {
                Step::Next(ep, s)
            } else {
                Step::Fail
            },
        )
    }

    fn back_reference(&mut self, p: usize, s: usize, digit: u8) -> Res<Step> {
        let l = digit as isize - b'1' as isize;
        if l < 0 || l as usize >= self.level || self.caps[l as usize].len == CAP_UNFINISHED {
            return Err(PatternError::new(format!(
                "invalid capture index %{}",
                l + 1
            )));
        }
        let cap = self.caps[l as usize];
        if cap.len == CAP_POSITION || self.src.len() - s < cap.len {
            return Ok(Step::Fail);
        }
        self.tick(cap.len as u64)?;
        let want = &self.src[cap.start..cap.start + cap.len];
        Ok(if &self.src[s..s + cap.len] == want {
            Step::Next(p + 2, s + cap.len)
        } else {
            Step::Fail
        })
    }

    /// A single-byte item (literal, `.`, `%x`, set) with its optional quantifier.
    fn item(&mut self, p: usize, s: usize) -> Res<Step> {
        let ep = self.class_end(p)?;
        let here = self.single_match(s, p, ep);
        Ok(match self.pat.get(ep) {
            Some(b'?') => {
                if here {
                    self.push(Frame::Skip { p: ep + 1, s })?;
                    Step::Next(ep + 1, s + 1)
                } else {
                    Step::Next(ep + 1, s)
                }
            }
            Some(b'+') => {
                if here {
                    self.expand(p, ep, s + 1)?
                } else {
                    Step::Fail
                }
            }
            Some(b'*') => self.expand(p, ep, s)?,
            Some(b'-') => {
                self.push(Frame::Lazy { item: p, ep, s })?;
                Step::Next(ep + 1, s)
            }
            _ => {
                if here {
                    Step::Next(ep, s + 1)
                } else {
                    Step::Fail
                }
            }
        })
    }

    /// Takes as many bytes as the item matches from `lo`, leaving shorter tries as a frame.
    fn expand(&mut self, p: usize, ep: usize, lo: usize) -> Res<Step> {
        let mut hi = lo;
        while self.single_match(hi, p, ep) {
            hi += 1;
        }
        self.tick((hi - lo) as u64)?;
        if hi > lo {
            self.push(Frame::Greedy { p: ep + 1, lo, hi })?;
        }
        Ok(Step::Next(ep + 1, hi))
    }

    /// End of the single-byte item starting at `p`.
    fn class_end(&self, mut p: usize) -> Res<usize> {
        let pat = self.pat;
        let c = pat[p];
        p += 1;
        if c == b'%' {
            if p >= pat.len() {
                return Err(PatternError::new("malformed pattern (ends with '%')"));
            }
            return Ok(p + 1);
        }
        if c == b'[' {
            if pat.get(p) == Some(&b'^') {
                p += 1;
            }
            loop {
                if p >= pat.len() {
                    return Err(PatternError::new("malformed pattern (missing ']')"));
                }
                let c = pat[p];
                p += 1;
                if c == b'%' && p < pat.len() {
                    p += 1;
                }
                if pat.get(p) == Some(&b']') {
                    return Ok(p + 1);
                }
            }
        }
        Ok(p)
    }

    fn single_match(&self, s: usize, p: usize, ep: usize) -> bool {
        let Some(&c) = self.src.get(s) else {
            return false;
        };
        match self.pat[p] {
            b'.' => true,
            b'%' => class_matches(c, self.pat[p + 1]),
            b'[' => self.in_set(c, p, ep - 1),
            lit => lit == c,
        }
    }

    /// Whether `c` is in the set whose `[` is at `p` and `]` at `ec`.
    fn in_set(&self, c: u8, mut p: usize, ec: usize) -> bool {
        let pat = self.pat;
        let mut sig = true;
        if pat[p + 1] == b'^' {
            sig = false;
            p += 1;
        }
        loop {
            p += 1;
            if p >= ec {
                return !sig;
            }
            if pat[p] == b'%' {
                p += 1;
                if class_matches(c, pat[p]) {
                    return sig;
                }
            } else if pat[p + 1] == b'-' && p + 2 < ec {
                p += 2;
                if pat[p - 2] <= c && c <= pat[p] {
                    return sig;
                }
            } else if pat[p] == c {
                return sig;
            }
        }
    }
}

/// Whether `c` is in class `%cl` (upper-case `cl` complements). Other `cl` mean the literal.
fn class_matches(c: u8, cl: u8) -> bool {
    let res = match cl.to_ascii_lowercase() {
        b'a' => c.is_ascii_alphabetic(),
        b'c' => c.is_ascii_control(),
        b'd' => c.is_ascii_digit(),
        b'g' => c.is_ascii_graphic(),
        b'l' => c.is_ascii_lowercase(),
        b'p' => c.is_ascii_punctuation(),
        b's' => matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'),
        b'u' => c.is_ascii_uppercase(),
        b'w' => c.is_ascii_alphanumeric(),
        b'x' => c.is_ascii_hexdigit(),
        _ => return cl == c,
    };
    if cl.is_ascii_uppercase() {
        !res
    } else {
        res
    }
}

/// Whether the pattern has no magic characters, so a plain search will do.
pub fn is_plain(pat: &[u8]) -> bool {
    !pat.iter().any(|c| b"^$*+?.([%-".contains(c))
}

/// Plain substring search from `init`, returning the match start. Work goes to `m`'s step count.
pub fn find_plain(m: &mut Matcher, hay: &[u8], needle: &[u8], init: usize) -> Res<Option<usize>> {
    if needle.is_empty() {
        return Ok(Some(init));
    }
    let mut i = init;
    while i + needle.len() <= hay.len() {
        m.tick(1)?;
        if hay[i] == needle[0] {
            m.tick(needle.len() as u64)?;
            if &hay[i..i + needle.len()] == needle {
                return Ok(Some(i));
            }
        }
        i += 1;
    }
    Ok(None)
}
