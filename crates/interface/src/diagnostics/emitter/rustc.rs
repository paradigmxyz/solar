//! Annotation collector for displaying diagnostics vendored from Rustc.

use crate::{
    diagnostics::{Level, MultiSpan, SpanLabel},
    source_map::{Loc, SourceFile},
    SourceMap,
};
use std::cmp::{max, min};
use sulk_data_structures::sync::Lrc;

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) line_index: usize,
    pub(crate) annotations: Vec<Annotation>,
}

impl Line {
    pub(crate) fn set_level(&mut self, level: Level) {
        for ann in &mut self.annotations {
            ann.level = Some(level);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Default)]
pub(crate) struct AnnotationColumn {
    /// the (0-indexed) column for *display* purposes, counted in characters, not utf-8 bytes
    pub(crate) display: usize,
    /// the (0-indexed) column in the file, counted in characters, not utf-8 bytes.
    ///
    /// this may be different from `self.display`,
    /// e.g. if the file contains hard tabs, because we convert tabs to spaces for error messages.
    ///
    /// for example:
    /// ```text
    /// (hard tab)hello
    ///           ^ this is display column 4, but file column 1
    /// ```
    ///
    /// we want to keep around the correct file offset so that column numbers in error messages
    /// are correct. (motivated by <https://github.com/rust-lang/rust/issues/109537>)
    pub(crate) file: usize,
}

impl AnnotationColumn {
    pub(crate) fn from_loc(loc: &Loc) -> Self {
        Self { display: loc.col_display, file: loc.col.0 }
    }
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct MultilineAnnotation {
    pub(crate) depth: usize,
    pub(crate) line_start: usize,
    pub(crate) line_end: usize,
    pub(crate) start_col: AnnotationColumn,
    pub(crate) end_col: AnnotationColumn,
    pub(crate) is_primary: bool,
    pub(crate) label: Option<String>,
    pub(crate) overlaps_exactly: bool,
}

impl MultilineAnnotation {
    pub(crate) fn increase_depth(&mut self) {
        self.depth += 1;
    }

    /// Compare two `MultilineAnnotation`s considering only the `Span` they cover.
    pub(crate) fn same_span(&self, other: &Self) -> bool {
        self.line_start == other.line_start
            && self.line_end == other.line_end
            && self.start_col == other.start_col
            && self.end_col == other.end_col
    }

    pub(crate) fn as_start(&self) -> Annotation {
        Annotation {
            start_col: self.start_col,
            end_col: AnnotationColumn {
                // these might not correspond to the same place anymore,
                // but that's okay for our purposes
                display: self.start_col.display + 1,
                file: self.start_col.file + 1,
            },
            is_primary: self.is_primary,
            label: None,
            annotation_type: AnnotationType::MultilineStart(self.depth),
            level: None,
        }
    }

    pub(crate) fn as_end(&self) -> Annotation {
        Annotation {
            start_col: AnnotationColumn {
                // these might not correspond to the same place anymore,
                // but that's okay for our purposes
                display: self.end_col.display.saturating_sub(1),
                file: self.end_col.file.saturating_sub(1),
            },
            end_col: self.end_col,
            is_primary: self.is_primary,
            label: self.label.clone(),
            annotation_type: AnnotationType::MultilineEnd(self.depth),
            level: None,
        }
    }

    pub(crate) fn as_line(&self) -> Annotation {
        Annotation {
            start_col: Default::default(),
            end_col: Default::default(),
            is_primary: self.is_primary,
            label: None,
            annotation_type: AnnotationType::MultilineLine(self.depth),
            level: None,
        }
    }
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) enum AnnotationType {
    /// Annotation under a single line of code
    Singleline,

    // The Multiline type above is replaced with the following three in order
    // to reuse the current label drawing code.
    //
    // Each of these corresponds to one part of the following diagram:
    //
    //     x |   foo(1 + bar(x,
    //       |  _________^              < MultilineStart
    //     x | |             y),        < MultilineLine
    //       | |______________^ label   < MultilineEnd
    //     x |       z);
    /// Annotation marking the first character of a fully shown multiline span
    MultilineStart(usize),
    /// Annotation marking the last character of a fully shown multiline span
    MultilineEnd(usize),
    /// Line at the left enclosing the lines of a fully shown multiline span
    // Just a placeholder for the drawing algorithm, to know that it shouldn't skip the first 4
    // and last 2 lines of code. The actual line is drawn in `emit_message_default` and not in
    // `draw_multiline_line`.
    MultilineLine(usize),
}

#[derive(Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct Annotation {
    /// Start column.
    /// Note that it is important that this field goes
    /// first, so that when we sort, we sort orderings by start
    /// column.
    pub(crate) start_col: AnnotationColumn,

    /// End column within the line (exclusive)
    pub(crate) end_col: AnnotationColumn,

    /// Is this annotation derived from primary span
    pub(crate) is_primary: bool,

    /// Optional label to display adjacent to the annotation.
    pub(crate) label: Option<String>,

    /// Is this a single line, multiline or multiline span minimized down to a
    /// smaller span.
    pub(crate) annotation_type: AnnotationType,

    pub(crate) level: Option<Level>,
}

#[derive(Debug)]
pub(crate) struct FileWithAnnotatedLines {
    pub(crate) file: Lrc<SourceFile>,
    pub(crate) lines: Vec<Line>,
    multiline_depth: usize,
}

impl FileWithAnnotatedLines {
    /// Preprocess all the annotations so that they are grouped by file and by line number
    /// This helps us quickly iterate over the whole message (including secondary file spans)
    pub(crate) fn collect_annotations(sm: &SourceMap, msp: &MultiSpan) -> Vec<Self> {
        fn add_annotation_to_file(
            file_vec: &mut Vec<FileWithAnnotatedLines>,
            file: Lrc<SourceFile>,
            line_index: usize,
            ann: Annotation,
        ) {
            for slot in file_vec.iter_mut() {
                // Look through each of our files for the one we're adding to
                if slot.file.name == file.name {
                    // See if we already have a line for it
                    for line_slot in &mut slot.lines {
                        if line_slot.line_index == line_index {
                            line_slot.annotations.push(ann);
                            return;
                        }
                    }
                    // We don't have a line yet, create one
                    slot.lines.push(Line { line_index, annotations: vec![ann] });
                    slot.lines.sort();
                    return;
                }
            }
            // This is the first time we're seeing the file
            file_vec.push(FileWithAnnotatedLines {
                file,
                lines: vec![Line { line_index, annotations: vec![ann] }],
                multiline_depth: 0,
            });
        }

        let mut output = vec![];
        let mut multiline_annotations = vec![];

        for SpanLabel { span, is_primary, label } in msp.span_labels() {
            // If we don't have a useful span, pick the primary span if that exists.
            // Worst case we'll just print an error at the top of the main file.
            let span = match (span.is_dummy(), msp.primary_span()) {
                (_, None) | (false, _) => span,
                (true, Some(span)) => span,
            };

            let lo = sm.lookup_char_pos(span.lo());
            let mut hi = sm.lookup_char_pos(span.hi());

            // Watch out for "empty spans". If we get a span like 6..6, we
            // want to just display a `^` at 6, so convert that to
            // 6..7. This is degenerate input, but it's best to degrade
            // gracefully -- and the parser likes to supply a span like
            // that for EOF, in particular.

            if lo.col_display == hi.col_display && lo.line == hi.line {
                hi.col_display += 1;
            }

            let label = label.as_ref().map(|m| m.as_str().to_string());

            if lo.line != hi.line {
                let ml = MultilineAnnotation {
                    depth: 1,
                    line_start: lo.line,
                    line_end: hi.line,
                    start_col: AnnotationColumn::from_loc(&lo),
                    end_col: AnnotationColumn::from_loc(&hi),
                    is_primary,
                    label,
                    overlaps_exactly: false,
                };
                multiline_annotations.push((lo.file, ml));
            } else {
                let ann = Annotation {
                    start_col: AnnotationColumn::from_loc(&lo),
                    end_col: AnnotationColumn::from_loc(&hi),
                    is_primary,
                    label,
                    annotation_type: AnnotationType::Singleline,
                    level: None,
                };
                add_annotation_to_file(&mut output, lo.file, lo.line, ann);
            };
        }

        // Find overlapping multiline annotations, put them at different depths
        multiline_annotations.sort_by_key(|(_, ml)| (ml.line_start, usize::MAX - ml.line_end));
        for (_, ann) in multiline_annotations.clone() {
            for (_, a) in multiline_annotations.iter_mut() {
                // Move all other multiline annotations overlapping with this one
                // one level to the right.
                if !(ann.same_span(a))
                    && num_overlap(ann.line_start, ann.line_end, a.line_start, a.line_end, true)
                {
                    a.increase_depth();
                } else if ann.same_span(a) && &ann != a {
                    a.overlaps_exactly = true;
                } else {
                    break;
                }
            }
        }

        let mut max_depth = 0; // max overlapping multiline spans
        for (_, ann) in &multiline_annotations {
            max_depth = max(max_depth, ann.depth);
        }
        // Change order of multispan depth to minimize the number of overlaps in the ASCII art.
        for (_, a) in multiline_annotations.iter_mut() {
            a.depth = max_depth - a.depth + 1;
        }
        for (file, ann) in multiline_annotations {
            let mut end_ann = ann.as_end();
            if !ann.overlaps_exactly {
                // avoid output like
                //
                //  |        foo(
                //  |   _____^
                //  |  |_____|
                //  | ||         bar,
                //  | ||     );
                //  | ||      ^
                //  | ||______|
                //  |  |______foo
                //  |         baz
                //
                // and instead get
                //
                //  |       foo(
                //  |  _____^
                //  | |         bar,
                //  | |     );
                //  | |      ^
                //  | |      |
                //  | |______foo
                //  |        baz
                add_annotation_to_file(&mut output, file.clone(), ann.line_start, ann.as_start());
                // 4 is the minimum vertical length of a multiline span when presented: two lines
                // of code and two lines of underline. This is not true for the special case where
                // the beginning doesn't have an underline, but the current logic seems to be
                // working correctly.
                let middle = min(ann.line_start + 4, ann.line_end);
                for line in ann.line_start + 1..middle {
                    // Every `|` that joins the beginning of the span (`___^`) to the end (`|__^`).
                    add_annotation_to_file(&mut output, file.clone(), line, ann.as_line());
                }
                let line_end = ann.line_end - 1;
                if middle < line_end {
                    add_annotation_to_file(&mut output, file.clone(), line_end, ann.as_line());
                }
            } else {
                end_ann.annotation_type = AnnotationType::Singleline;
            }
            add_annotation_to_file(&mut output, file, ann.line_end, end_ann);
        }
        for file_vec in output.iter_mut() {
            file_vec.multiline_depth = max_depth;
        }
        output
    }

    pub(crate) fn set_level(&mut self, level: Level) {
        for line in &mut self.lines {
            line.set_level(level);
        }
    }

    pub(crate) fn add_lines(&mut self, lines: impl IntoIterator<Item = Line>) {
        fn is_sorted(lines: &[Line]) -> bool {
            lines.windows(2).all(|ls| ls[0] < ls[1])
        }

        debug_assert!(is_sorted(&self.lines), "file lines should be sorted");
        for line in lines {
            match self.lines.binary_search_by_key(&line.line_index, |l| l.line_index) {
                Ok(i) => {
                    self.lines[i].annotations.extend(line.annotations);
                    self.lines[i].annotations.sort();
                }
                Err(i) => {
                    self.lines.insert(i, line);
                }
            }
        }
        debug_assert!(is_sorted(&self.lines), "file lines should still be sorted");
    }
}

fn num_overlap(
    a_start: usize,
    a_end: usize,
    b_start: usize,
    b_end: usize,
    inclusive: bool,
) -> bool {
    let extra = if inclusive { 1 } else { 0 };
    (b_start..b_end + extra).contains(&a_start) || (a_start..a_end + extra).contains(&b_start)
}
