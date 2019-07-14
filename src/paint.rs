use std::cmp::max;
use std::io::Write;
use std::iter::Peekable;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, StyleModifier};
use syntect::parsing::SyntaxReference;

use crate::config;
use crate::paint::superimpose_style_sections::superimpose_style_sections;
use crate::style;

pub struct Painter<'a> {
    pub minus_lines: Vec<String>,
    pub plus_lines: Vec<String>,

    // TODO: store slice references instead of creating Strings
    pub minus_line_style_sections: Vec<Vec<(StyleModifier, String)>>,
    pub plus_line_style_sections: Vec<Vec<(StyleModifier, String)>>,

    pub writer: &'a mut Write,
    pub syntax: Option<&'a SyntaxReference>,
    pub highlighter: HighlightLines<'a>,
    pub config: &'a config::Config<'a>,
    pub output_buffer: String,
}

impl<'a> Painter<'a> {
    pub fn reset_highlighter(&mut self) {
        self.highlighter = HighlightLines::new(self.syntax.unwrap(), self.config.theme);
    }

    pub fn paint_buffered_lines(&mut self) {
        self.set_background_style_sections();
        // TODO: lines and style sections contain identical line text
        if self.minus_lines.len() > 0 {
            self.paint_lines(
                // TODO: don't clone
                self.minus_lines.iter().cloned().collect(),
                self.minus_line_style_sections.iter().cloned().collect(),
                self.config.opt.highlight_removed,
            );
            self.minus_lines.clear();
            self.minus_line_style_sections.clear();
        }
        if self.plus_lines.len() > 0 {
            self.paint_lines(
                // TODO: don't clone
                self.plus_lines.iter().cloned().collect(),
                self.plus_line_style_sections.iter().cloned().collect(),
                true,
            );
            self.plus_lines.clear();
            self.plus_line_style_sections.clear();
        }
    }

    /// Superimpose background styles and foreground syntax
    /// highlighting styles, and write colored lines to output buffer.
    pub fn paint_lines(
        &mut self,
        lines: Vec<String>,
        line_style_sections: Vec<Vec<(StyleModifier, String)>>,
        syntax_highlight: bool,
    ) {
        for (line, style_sections) in lines.iter().zip(line_style_sections) {
            let syntax_highlighting_style_sections: Vec<(Style, String)> = match syntax_highlight {
                true => self
                    .highlighter
                    .highlight(&line, &self.config.syntax_set)
                    .iter()
                    .map(|(style, s)| (*style, s.to_string()))
                    .collect::<Vec<(Style, String)>>(),
                false => vec![(self.config.no_style, line.to_string())],
            };
            let superimposed_style_sections =
                superimpose_style_sections(syntax_highlighting_style_sections, style_sections);
            for (style, text) in superimposed_style_sections {
                paint_section(&text, style, &mut self.output_buffer).unwrap();
            }
            self.output_buffer.push_str("\n");
        }
    }

    /// Write output buffer to output stream, and clear the buffer.
    pub fn emit(&mut self) -> std::io::Result<()> {
        write!(self.writer, "{}", self.output_buffer)?;
        self.output_buffer.truncate(0);
        Ok(())
    }

    /// Set background styles for minus and plus lines in buffer.
    fn set_background_style_sections(&mut self) {
        if self.minus_lines.len() == self.plus_lines.len() {
            self.set_background_style_sections_diff_detail();
        } else {
            self.set_background_style_sections_plain();
        }
    }

    fn set_background_style_sections_plain(&mut self) {
        for line in self.minus_lines.iter() {
            self.minus_line_style_sections
                .push(vec![(self.config.minus_style_modifier, line.to_string())]);
        }
        for line in self.plus_lines.iter() {
            self.plus_line_style_sections
                .push(vec![(self.config.plus_style_modifier, line.to_string())]);
        }
    }

    /// Create background style sections for a region of removed/added lines.
    /*
      This function is called iff a region of n minus lines followed
      by n plus lines is encountered, e.g. n successive lines have
      been partially changed.

      Consider the i-th such line and let m, p be the i-th minus and
      i-th plus line, respectively.  The following cases exist:

      1. Whitespace deleted at line beginning.
         => The deleted section is highlighted in m; p is unstyled.

      2. Whitespace inserted at line beginning.
         => The inserted section is highlighted in p; m is unstyled.

      3. An internal section of the line containing a non-whitespace character has been deleted.
         => The deleted section is highlighted in m; p is unstyled.

      4. An internal section of the line containing a non-whitespace character has been changed.
         => The original section is highlighted in m; the replacement is highlighted in p.

      5. An internal section of the line containing a non-whitespace character has been inserted.
         => The inserted section is highlighted in p; m is unstyled.

      Note that whitespace can be neither deleted nor inserted at the
      end of the line: the line by definition has no trailing
      whitespace.
    */
    fn set_background_style_sections_diff_detail(&mut self) {
        for (minus, plus) in self.minus_lines.iter().zip(self.plus_lines.iter()) {
            let string_pair = StringPair::new(minus, plus);
            let change_begin = string_pair.common_prefix_length;

            // We require that (right-trimmed length) >= (common prefix length). Consider:
            // minus = "a    "
            // plus  = "a b  "
            // Here, the right-trimmed length of minus is 1, yet the common prefix length is
            // 2. We resolve this by taking the following maxima:
            let minus_length = max(string_pair.lengths[0], string_pair.common_prefix_length);
            let plus_length = max(string_pair.lengths[1], string_pair.common_prefix_length);

            // We require that change_begin <= change_end. Consider:
            // minus = "a c"
            // plus  = "a b c"
            // Here, the common prefix length is 2, and the common suffix length is 2, yet the
            // length of minus is 3. This overlap between prefix and suffix leads to a violation of
            // the requirement. We resolve this by taking the following maxima:
            let minus_change_end = max(
                minus_length - string_pair.common_suffix_length,
                change_begin,
            );
            let plus_change_end = max(plus_length - string_pair.common_suffix_length, change_begin);

            self.minus_line_style_sections.push(vec![
                (
                    self.config.minus_style_modifier,
                    minus[0..change_begin].to_string(),
                ),
                (
                    self.config.minus_emph_style_modifier,
                    minus[change_begin..minus_change_end].to_string(),
                ),
                (
                    self.config.minus_style_modifier,
                    minus[minus_change_end..].to_string(),
                ),
            ]);
            self.plus_line_style_sections.push(vec![
                (
                    self.config.plus_style_modifier,
                    plus[0..change_begin].to_string(),
                ),
                (
                    self.config.plus_emph_style_modifier,
                    plus[change_begin..plus_change_end].to_string(),
                ),
                (
                    self.config.plus_style_modifier,
                    plus[plus_change_end..].to_string(),
                ),
            ]);
        }
    }
}

/// A pair of right-trimmed strings.
struct StringPair {
    common_prefix_length: usize,
    common_suffix_length: usize,
    lengths: [usize; 2],
}

impl StringPair {
    pub fn new(s0: &str, s1: &str) -> StringPair {
        let common_prefix_length = StringPair::common_prefix_length(s0.chars(), s1.chars());
        let (common_suffix_length, trailing_whitespace) =
            StringPair::suffix_data(s0.chars(), s1.chars());
        StringPair {
            common_prefix_length,
            common_suffix_length,
            lengths: [
                s0.len() - trailing_whitespace[0],
                s1.len() - trailing_whitespace[1],
            ],
        }
    }

    fn common_prefix_length(
        s0: impl Iterator<Item = char>,
        s1: impl Iterator<Item = char>,
    ) -> usize {
        let mut i = 0;
        for (c0, c1) in s0.zip(s1) {
            if c0 != c1 {
                break;
            } else {
                i += 1;
            }
        }
        i
    }

    /// Return common suffix length and number of trailing whitespace characters on each string.
    fn suffix_data(
        s0: impl DoubleEndedIterator<Item = char>,
        s1: impl DoubleEndedIterator<Item = char>,
    ) -> (usize, [usize; 2]) {
        let mut s0 = s0.rev().peekable();
        let mut s1 = s1.rev().peekable();
        let n0 = StringPair::consume_whitespace(&mut s0);
        let n1 = StringPair::consume_whitespace(&mut s1);

        (StringPair::common_prefix_length(s0, s1), [n0, n1])
    }

    /// Consume leading whitespace; return number of characters consumed.
    fn consume_whitespace(s: &mut Peekable<impl Iterator<Item = char>>) -> usize {
        let mut i = 0;
        loop {
            match s.peek() {
                Some(' ') => {
                    s.next();
                    i += 1;
                }
                _ => break,
            }
        }
        i
    }
}

/// Write section text to buffer with color escape codes.
fn paint_section(text: &str, style: Style, output_buffer: &mut String) -> std::fmt::Result {
    use std::fmt::Write;
    match style.background {
        style::NO_COLOR => (),
        _ => write!(
            output_buffer,
            "\x1b[48;2;{};{};{}m",
            style.background.r, style.background.g, style.background.b
        )?,
    }
    match style.foreground {
        style::NO_COLOR => write!(output_buffer, "{}", text)?,
        _ => write!(
            output_buffer,
            "\x1b[38;2;{};{};{}m{}",
            style.foreground.r, style.foreground.g, style.foreground.b, text
        )?,
    };
    Ok(())
}

mod superimpose_style_sections {
    use syntect::highlighting::{Style, StyleModifier};

    pub fn superimpose_style_sections(
        sections_1: Vec<(Style, String)>,
        sections_2: Vec<(StyleModifier, String)>,
    ) -> Vec<(Style, String)> {
        coalesce(superimpose(
            explode(sections_1)
                .iter()
                .zip(explode(sections_2))
                .collect::<Vec<(&(Style, char), (StyleModifier, char))>>(),
        ))
    }

    fn explode<T>(style_sections: Vec<(T, String)>) -> Vec<(T, char)>
    where
        T: Copy,
    {
        let mut exploded: Vec<(T, char)> = Vec::new();
        for (style, string) in style_sections {
            for c in string.chars() {
                exploded.push((style, c));
            }
        }
        exploded
    }

    fn superimpose(
        style_section_pairs: Vec<(&(Style, char), (StyleModifier, char))>,
    ) -> Vec<(Style, char)> {
        let mut superimposed: Vec<(Style, char)> = Vec::new();
        for ((style, char_1), (modifier, char_2)) in style_section_pairs {
            if *char_1 != char_2 {
                panic!(
                    "String mismatch encountered while superimposing style sections: '{}' vs '{}'",
                    *char_1, char_2
                )
            }
            superimposed.push((style.apply(modifier), *char_1));
        }
        superimposed
    }

    fn coalesce(style_sections: Vec<(Style, char)>) -> Vec<(Style, String)> {
        let mut coalesced: Vec<(Style, String)> = Vec::new();
        let mut style_sections = style_sections.iter();
        match style_sections.next() {
            Some((style, c)) => {
                let mut current_string = c.to_string();
                let mut current_style = style;
                for (style, c) in style_sections {
                    if style != current_style {
                        coalesced.push((*current_style, current_string));
                        current_string = String::new();
                        current_style = style;
                    }
                    current_string.push(*c);
                }
                coalesced.push((*current_style, current_string));
            }
            None => (),
        }
        coalesced
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use syntect::highlighting::{Color, FontStyle, Style, StyleModifier};

        const STYLE: Style = Style {
            foreground: Color::BLACK,
            background: Color::BLACK,
            font_style: FontStyle::BOLD,
        };
        const STYLE_MODIFIER: StyleModifier = StyleModifier {
            foreground: Some(Color::WHITE),
            background: Some(Color::WHITE),
            font_style: Some(FontStyle::UNDERLINE),
        };
        const SUPERIMPOSED_STYLE: Style = Style {
            foreground: Color::WHITE,
            background: Color::WHITE,
            font_style: FontStyle::UNDERLINE,
        };

        #[test]
        fn test_superimpose_style_sections_1() {
            let string = String::from("ab");
            let sections_1 = vec![(STYLE, string.clone())];
            let sections_2 = vec![(STYLE_MODIFIER, string.clone())];
            let superimposed = vec![(SUPERIMPOSED_STYLE, string.clone())];
            assert_eq!(
                superimpose_style_sections(sections_1, sections_2),
                superimposed
            );
        }

        #[test]
        fn test_superimpose_style_sections_2() {
            let sections_1 = vec![(STYLE, String::from("ab"))];
            let sections_2 = vec![
                (STYLE_MODIFIER, String::from("a")),
                (STYLE_MODIFIER, String::from("b")),
            ];
            let superimposed = vec![(SUPERIMPOSED_STYLE, String::from("ab"))];
            assert_eq!(
                superimpose_style_sections(sections_1, sections_2),
                superimposed
            );
        }

        #[test]
        fn test_explode() {
            let arbitrary = 0;
            let string = String::from("ab");
            assert_eq!(
                explode(vec![(arbitrary, string)]),
                vec![(arbitrary, 'a'), (arbitrary, 'b')]
            )
        }

        #[test]
        fn test_superimpose() {
            let x = (STYLE, 'a');
            let pairs = vec![(&x, (STYLE_MODIFIER, 'a'))];
            assert_eq!(superimpose(pairs), vec![(SUPERIMPOSED_STYLE, 'a')]);
        }
    }

}

#[cfg(test)]
mod tests {
    fn common_prefix_length(s1: &str, s2: &str) -> usize {
        super::StringPair::new(s1, s2).common_prefix_length
    }

    fn common_suffix_length(s1: &str, s2: &str) -> usize {
        super::StringPair::new(s1, s2).common_suffix_length
    }

    #[test]
    fn test_common_prefix_length() {
        assert_eq!(common_prefix_length("", ""), 0);
        assert_eq!(common_prefix_length("", "a"), 0);
        assert_eq!(common_prefix_length("a", ""), 0);
        assert_eq!(common_prefix_length("a", "b"), 0);
        assert_eq!(common_prefix_length("a", "a"), 1);
        assert_eq!(common_prefix_length("a", "ab"), 1);
        assert_eq!(common_prefix_length("ab", "a"), 1);
        assert_eq!(common_prefix_length("ab", "aba"), 2);
        assert_eq!(common_prefix_length("aba", "ab"), 2);
    }

    #[test]
    fn test_common_prefix_length_with_leading_whitespace() {
        assert_eq!(common_prefix_length(" ", ""), 0);
        assert_eq!(common_prefix_length(" ", " "), 1);
        assert_eq!(common_prefix_length(" a", " a"), 2);
        assert_eq!(common_prefix_length(" a", "a"), 0);
    }

    #[test]
    fn test_common_suffix_length() {
        assert_eq!(common_suffix_length("", ""), 0);
        assert_eq!(common_suffix_length("", "a"), 0);
        assert_eq!(common_suffix_length("a", ""), 0);
        assert_eq!(common_suffix_length("a", "b"), 0);
        assert_eq!(common_suffix_length("a", "a"), 1);
        assert_eq!(common_suffix_length("a", "ab"), 0);
        assert_eq!(common_suffix_length("ab", "a"), 0);
        assert_eq!(common_suffix_length("ab", "b"), 1);
        assert_eq!(common_suffix_length("ab", "aab"), 2);
        assert_eq!(common_suffix_length("aba", "ba"), 2);
    }

    #[test]
    fn test_common_suffix_length_with_trailing_whitespace() {
        assert_eq!(common_suffix_length("", "  "), 0);
        assert_eq!(common_suffix_length("  ", "a"), 0);
        assert_eq!(common_suffix_length("a  ", ""), 0);
        assert_eq!(common_suffix_length("a", "b  "), 0);
        assert_eq!(common_suffix_length("a", "a  "), 1);
        assert_eq!(common_suffix_length("a  ", "ab  "), 0);
        assert_eq!(common_suffix_length("ab", "a  "), 0);
        assert_eq!(common_suffix_length("ab  ", "b "), 1);
        assert_eq!(common_suffix_length("ab ", "aab  "), 2);
        assert_eq!(common_suffix_length("aba ", "ba"), 2);
    }
}
