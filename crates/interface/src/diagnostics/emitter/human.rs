use super::{io_panic, rustc::FileWithAnnotatedLines, Diagnostic, Emitter};
use crate::{
    diagnostics::{Level, MultiSpan, Style, SubDiagnostic},
    source_map::SourceFile,
    SourceMap,
};
use annotate_snippets::{Annotation, AnnotationType, Renderer, Slice, Snippet, SourceAnnotation};
use anstream::{AutoStream, ColorChoice};
use std::{
    borrow::Cow,
    io::{self, Write},
};
use sulk_data_structures::sync::Lrc;

const DEFAULT_RENDERER: Renderer = Renderer::plain()
    .error(Level::Error.style())
    .warning(Level::Warning.style())
    .info(Level::Note.style())
    .note(Level::Note.style())
    .help(Level::Help.style())
    .line_no(Style::LineNumber.to_color_spec(Level::Note))
    .emphasis(anstyle::Style::new().bold())
    .none(anstyle::Style::new());

/// Diagnostic emitter that emits to an arbitrary [`io::Write`] writer in human-readable format.
pub struct HumanEmitter {
    writer: AutoStream<Box<dyn Write>>,
    source_map: Option<Lrc<SourceMap>>,
    renderer: Renderer,
}

impl Emitter for HumanEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
        self.snippet(diagnostic, |this, snippet| {
            writeln!(this.writer, "{}\n", this.renderer.render(snippet))?;
            this.writer.flush()
        })
        .unwrap_or_else(|e| io_panic(e));
    }

    fn source_map(&self) -> Option<&Lrc<SourceMap>> {
        self.source_map.as_ref()
    }

    fn supports_color(&self) -> bool {
        match self.writer.current_choice() {
            ColorChoice::AlwaysAnsi | ColorChoice::Always => true,
            ColorChoice::Auto | ColorChoice::Never => false,
        }
    }
}

impl HumanEmitter {
    /// Creates a new `HumanEmitter` that writes to given writer.
    ///
    /// Note that a color choice of `Auto` will be treated as `Never` because the writer opaque
    /// at this point. Prefer calling [`AutoStream::choice`] on the writer if it is known
    /// before-hand.
    pub fn new(writer: Box<dyn Write>, color: ColorChoice) -> Self {
        Self {
            writer: AutoStream::new(writer, color),
            source_map: None,
            renderer: DEFAULT_RENDERER,
        }
    }

    /// Creates a new `HumanEmitter` that writes to stderr, for use in tests.
    pub fn test(ui: bool) -> Self {
        struct TestWriter(io::Stderr);

        impl Write for TestWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                // The main difference between `stderr`: use the `eprint!` macro so that the output
                // can get captured by the test harness.
                eprint!("{}", String::from_utf8_lossy(buf));
                Ok(buf.len())
            }

            fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
                self.write(buf).map(drop)
            }

            fn flush(&mut self) -> io::Result<()> {
                self.0.flush()
            }
        }

        if ui {
            Self::stderr(ColorChoice::Never).anonymized_line_numbers(true)
        } else {
            Self::new(Box::new(TestWriter(io::stderr())), ColorChoice::Always)
        }
    }

    /// Creates a new `HumanEmitter` that writes to stderr.
    pub fn stderr(mut color_choice: ColorChoice) -> Self {
        let stderr = io::stderr();
        // Call `AutoStream::choice` on `io::Stderr` rather than later on `Box<dyn Write>`.
        if color_choice == ColorChoice::Auto {
            color_choice = AutoStream::choice(&stderr);
        }
        Self::new(Box::new(stderr), color_choice)
    }

    /// Sets the source map.
    pub fn source_map(mut self, source_map: Option<Lrc<SourceMap>>) -> Self {
        self.source_map = source_map;
        self
    }

    /// Sets whether to anonymize line numbers.
    pub fn anonymized_line_numbers(mut self, anonymized_line_numbers: bool) -> Self {
        self.renderer = self.renderer.anonymized_line_numbers(anonymized_line_numbers);
        self
    }

    /// Formats the given `diagnostic` into a [`Snippet`] suitable for use with the renderer.
    fn snippet<R>(
        &mut self,
        diagnostic: &Diagnostic,
        f: impl FnOnce(&mut Self, Snippet<'_>) -> R,
    ) -> R {
        // Current format (annotate-snippets 0.10.0) (comments in <...>):
        /*
        title.annotation_type[title.id]: title.label
           --> slices[0].origin
            |
         LL | slices[0].source[ann[0].range] <ann = slices[0].annotations>
            | ^^^^^^^^^^^^^^^^ ann[0].annotation_type: ann[0].label <type is skipped for error, warning>
         LL | slices[0].source[ann[1].range]
            | ---------------- ann[1].annotation_type: ann[1].label
            |
           ::: slices[1].origin
            |
        etc...
            |
            = footer[0].annotation_type: footer[0].label <I believe the .id here is always ignored>
            = footer[1].annotation_type: footer[1].label
            = ...
        */

        let title = OwnedAnnotation::from_diagnostic(diagnostic);

        let owned_slices = self
            .source_map
            .as_deref()
            .map(|sm| OwnedSlice::collect(sm, diagnostic))
            .unwrap_or_default();

        // Dummy subdiagnostics go in the footer, while non-dummy ones go in the slices.
        let dummy_subs: Vec<_> = diagnostic
            .children
            .iter()
            .filter(|sub| sub.span.is_dummy())
            .map(OwnedAnnotation::from_subdiagnostic)
            .collect();

        let snippet = Snippet {
            title: Some(title.as_ref()),
            slices: owned_slices.iter().map(OwnedSlice::as_ref).collect(),
            footer: dummy_subs.iter().map(OwnedAnnotation::as_ref).collect(),
        };
        f(self, snippet)
    }
}

#[derive(Debug)]
struct OwnedAnnotation {
    id: Option<String>,
    label: Option<String>,
    annotation_type: AnnotationType,
}

impl OwnedAnnotation {
    fn from_diagnostic(diag: &Diagnostic) -> Self {
        Self {
            id: diag.code.as_ref().map(|s| s.id.to_string()),
            label: Some(diag.label().into_owned()),
            annotation_type: to_annotation_type(diag.level),
        }
    }

    fn from_subdiagnostic(sub: &SubDiagnostic) -> Self {
        Self {
            id: None,
            label: Some(sub.label().into_owned()),
            annotation_type: to_annotation_type(sub.level),
        }
    }

    fn as_ref(&self) -> Annotation<'_> {
        Annotation {
            id: self.id.as_deref(),
            label: self.label.as_deref(),
            annotation_type: self.annotation_type,
        }
    }
}

#[derive(Debug)]
struct OwnedSourceAnnotation {
    range: (usize, usize),
    label: String,
    annotation_type: AnnotationType,
}

impl OwnedSourceAnnotation {
    fn as_ref(&self) -> SourceAnnotation<'_> {
        SourceAnnotation {
            range: self.range,
            label: &self.label,
            annotation_type: self.annotation_type,
        }
    }
}

#[derive(Debug)]
struct OwnedSlice {
    origin: Option<String>,
    source: String,
    line_start: usize,
    fold: bool,
    annotations: Vec<OwnedSourceAnnotation>,
}

impl OwnedSlice {
    fn collect(sm: &SourceMap, diagnostic: &Diagnostic) -> Vec<Self> {
        // Collect main diagnostic.
        let mut files = Self::collect_files(sm, &diagnostic.span);
        files.iter_mut().for_each(|file| file.set_level(diagnostic.level));

        // Collect subdiagnostics.
        for sub in &diagnostic.children {
            let label = sub.label();
            for mut sub_file in Self::collect_files(sm, &sub.span) {
                for line in &mut sub_file.lines {
                    for ann in &mut line.annotations {
                        ann.level = Some(sub.level);
                        // TODO: Is this right?
                        if ann.is_primary && ann.label.is_none() {
                            ann.label = Some(label.to_string());
                        }
                    }
                }

                if let Some(main_file) =
                    files.iter_mut().find(|main_file| Lrc::ptr_eq(&main_file.file, &sub_file.file))
                {
                    main_file.add_lines(sub_file.lines);
                } else {
                    files.push(sub_file);
                }
            }
        }

        files
            .iter()
            .map(|file| file_to_slice(sm, &file.file, &file.lines, diagnostic.level))
            .collect()
    }

    fn collect_files(sm: &SourceMap, msp: &MultiSpan) -> Vec<FileWithAnnotatedLines> {
        let mut annotated_files = FileWithAnnotatedLines::collect_annotations(sm, msp);
        if let Some(primary_span) = msp.primary_span() {
            if !primary_span.is_dummy() && annotated_files.len() > 1 {
                let primary_lo = sm.lookup_char_pos(primary_span.lo());
                if let Ok(pos) =
                    annotated_files.binary_search_by(|x| x.file.name.cmp(&primary_lo.file.name))
                {
                    annotated_files.swap(0, pos);
                }
            }
        }
        annotated_files
    }

    fn as_ref(&self) -> Slice<'_> {
        Slice {
            source: &self.source,
            line_start: self.line_start,
            origin: self.origin.as_deref(),
            fold: self.fold,
            annotations: self.annotations.iter().map(OwnedSourceAnnotation::as_ref).collect(),
        }
    }
}

/// Merges back multi-line annotations that were split across multiple lines into a single
/// annotation that's suitable for `annotate-snippets`.
///
/// Assumes that lines are sorted.
fn file_to_slice(
    sm: &SourceMap,
    file: &SourceFile,
    lines: &[super::rustc::Line],
    default_level: Level,
) -> OwnedSlice {
    debug_assert!(!lines.is_empty());

    let first_line = lines.first().unwrap().line_index;
    let last_line = lines.last().unwrap().line_index;
    debug_assert!(last_line >= first_line);

    let mut snippet =
        Cow::Borrowed(file.get_lines(first_line - 1..=last_line - 1).unwrap_or_default());
    if snippet.contains('\t') {
        snippet = Cow::Owned(snippet.replace('\t', "    "));
    }

    let mut slice = OwnedSlice {
        origin: Some(sm.filename_for_diagnostics(&file.name).to_string()),
        source: snippet.into_owned(),
        line_start: first_line,
        fold: true,
        annotations: Vec::new(),
    };
    let mut multiline_start = None;
    let mut current_i = 0;
    let mut prev_line_no = first_line - 1;
    for line in lines {
        // Returns the position of the given column in the local snippet.
        let get_pos = |c: &super::rustc::AnnotationColumn| current_i + c.display;

        for ann in &line.annotations {
            match ann.annotation_type {
                super::rustc::AnnotationType::Singleline => {
                    slice.annotations.push(OwnedSourceAnnotation {
                        range: (get_pos(&ann.start_col), get_pos(&ann.end_col)),
                        label: ann.label.clone().unwrap_or_default(),
                        annotation_type: to_annotation_type(ann.level.unwrap_or(default_level)),
                    })
                }
                super::rustc::AnnotationType::MultilineStart(_) => {
                    debug_assert!(multiline_start.is_none());
                    multiline_start = Some((ann.label.as_ref(), get_pos(&ann.start_col)));
                }
                super::rustc::AnnotationType::MultilineLine(_) => {}
                super::rustc::AnnotationType::MultilineEnd(_) => {
                    let (label, multiline_start_idx) = multiline_start.take().unwrap();
                    let end_idx = get_pos(&ann.end_col);
                    debug_assert!(end_idx >= multiline_start_idx);
                    slice.annotations.push(OwnedSourceAnnotation {
                        range: (multiline_start_idx, end_idx),
                        label: label.or(ann.label.as_ref()).cloned().unwrap_or_default(),
                        annotation_type: to_annotation_type(ann.level.unwrap_or(default_level)),
                    });
                }
            }
        }

        if let Some(i) = line.line_index.checked_sub(1) {
            if i >= prev_line_no {
                current_i += file
                    .get_lines(prev_line_no..=i)
                    .unwrap_or("")
                    .chars()
                    .map(|c| if c == '\t' { 4 } else { 1 })
                    .sum::<usize>()
                    + 1;
            }
        }

        prev_line_no = line.line_index;
    }
    // dbg!(lines, &slice);
    slice
}

fn to_annotation_type(level: Level) -> AnnotationType {
    match level {
        Level::Bug | Level::Fatal | Level::Error => AnnotationType::Error,
        Level::Warning => AnnotationType::Warning,
        Level::Note | Level::OnceNote | Level::FailureNote => AnnotationType::Note,
        Level::Help | Level::OnceHelp => AnnotationType::Help,
        Level::Allow => AnnotationType::Info,
    }
}
