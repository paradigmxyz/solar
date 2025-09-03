use super::{Diag, Emitter, io_panic, rustc::FileWithAnnotatedLines};
use crate::{
    SourceMap,
    diagnostics::{Level, MultiSpan, Style, SubDiagnostic, SuggestionStyle},
    source_map::SourceFile,
};
use annotate_snippets::{
    Annotation, AnnotationKind, Group, Level as ASLevel, Message, Patch, Renderer, Report, Snippet,
    Title,
};
use anstream::{AutoStream, ColorChoice};
use std::{
    any::Any,
    borrow::Cow,
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, OnceLock},
};

// TODO: Tabs are not formatted correctly: https://github.com/rust-lang/annotate-snippets-rs/issues/25

type Writer = dyn Write + Send + 'static;

const DEFAULT_RENDERER: Renderer = Renderer::styled()
    .error(Level::Error.style())
    .warning(Level::Warning.style())
    .note(Level::Note.style())
    .help(Level::Help.style())
    .line_num(Style::LineNumber.to_color_spec(Level::Note))
    .addition(Style::Addition.to_color_spec(Level::Note))
    .removal(Style::Removal.to_color_spec(Level::Note))
    .context(Style::LabelSecondary.to_color_spec(Level::Note));

/// Diagnostic emitter that emits to an arbitrary [`io::Write`] writer in human-readable format.
pub struct HumanEmitter {
    writer_type_id: std::any::TypeId,
    real_writer: *mut Writer,
    writer: AutoStream<Box<Writer>>,
    source_map: Option<Arc<SourceMap>>,
    renderer: Renderer,
}

// SAFETY: `real_writer` always points to the `Writer` in `writer`.
unsafe impl Send for HumanEmitter {}

impl Emitter for HumanEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &mut Diag) {
        self.snippet(diagnostic, |this, snippet| {
            writeln!(this.writer, "{}\n", this.renderer.render(snippet))?;
            this.writer.flush()
        })
        .unwrap_or_else(|e| io_panic(e));
    }

    fn source_map(&self) -> Option<&Arc<SourceMap>> {
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
    pub fn new<W: Write + Send + 'static>(writer: W, color: ColorChoice) -> Self {
        let writer_type_id = writer.type_id();
        let mut real_writer = Box::new(writer) as Box<Writer>;
        Self {
            writer_type_id,
            real_writer: &mut *real_writer,
            writer: AutoStream::new(real_writer, color),
            source_map: None,
            renderer: DEFAULT_RENDERER,
        }
    }

    /// Creates a new `HumanEmitter` that writes to stderr, for use in tests.
    pub fn test() -> Self {
        struct TestWriter;

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
                io::stderr().flush()
            }
        }

        Self::new(TestWriter, ColorChoice::Always)
    }

    /// Creates a new `HumanEmitter` that writes to stderr.
    pub fn stderr(color_choice: ColorChoice) -> Self {
        // `io::Stderr` is not buffered.
        Self::new(io::BufWriter::new(io::stderr()), stderr_choice(color_choice))
    }

    /// Sets the source map.
    pub fn source_map(mut self, source_map: Option<Arc<SourceMap>>) -> Self {
        self.set_source_map(source_map);
        self
    }

    /// Sets the source map.
    pub fn set_source_map(&mut self, source_map: Option<Arc<SourceMap>>) {
        self.source_map = source_map;
    }

    /// Sets whether to emit diagnostics in a way that is suitable for UI testing.
    pub fn ui_testing(mut self, yes: bool) -> Self {
        self.renderer = self.renderer.anonymized_line_numbers(yes);
        self
    }

    /// Sets whether to emit diagnostics in a way that is suitable for UI testing.
    pub fn set_ui_testing(&mut self, yes: bool) {
        self.renderer =
            std::mem::replace(&mut self.renderer, DEFAULT_RENDERER).anonymized_line_numbers(yes);
    }

    /// Downcasts the underlying writer to the specified type.
    fn downcast_writer<T: Any>(&self) -> Option<&T> {
        if self.writer_type_id == std::any::TypeId::of::<T>() {
            Some(unsafe { &*(self.real_writer as *const T) })
        } else {
            None
        }
    }

    /// Downcasts the underlying writer to the specified type.
    fn downcast_writer_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.writer_type_id == std::any::TypeId::of::<T>() {
            Some(unsafe { &mut *(self.real_writer as *mut T) })
        } else {
            None
        }
    }

    /// Formats the given `diagnostic` into a [`Message`] suitable for use with the renderer.
    fn snippet<R>(
        &mut self,
        diagnostic: &mut Diag,
        f: impl FnOnce(&mut Self, Report<'_>) -> R,
    ) -> R {
        // Current format (annotate-snippets 0.12.0) (comments in <...>):
        /*
        title.level[title.id]: title.label
           --> snippets[0].path:ll:cc
            |
         LL | snippets[0].source[ann[0].range] <ann = snippets[0].annotations; these are diag.span_label()s>
            | ^^^^^^^^^^^^^^^^ ann[0].label <primary>
         LL | snippets[0].source[ann[1].range]
            | ---------------- ann[1].label <secondary>
            |
           ::: snippets[1].path:ll:cc
            |
        etc...
            |
            = footer[0].level: footer[0].label
            = footer[1].level: footer[1].label
            = ...
        <other groups for subdiagnostics, same as above without footers>
        */

        // Process suggestions. Inline primary span if necessary.
        let mut primary_span = Cow::Borrowed(&diagnostic.span);
        self.primary_span_formatted(&mut primary_span, &mut diagnostic.suggestions);

        // Render suggestions unless style is `HideCodeAlways`.
        // Note that if the span was previously inlined, suggestions will be empty.
        let children = diagnostic
            .suggestions
            .iter()
            .filter(|sugg| sugg.style != SuggestionStyle::HideCodeAlways)
            .collect::<Vec<_>>();

        let sm = self.source_map.as_deref();
        let title = title_from_diagnostic(diagnostic);
        let snippets = sm.map(|sm| iter_snippets(sm, &primary_span)).into_iter().flatten();

        // Dummy subdiagnostics go in the main group's footer, non-dummy ones go as separate groups.
        let subs = |d| diagnostic.children.iter().filter(move |sub| sub.span.is_dummy() == d);
        let footers = subs(true).map(|sub| message_from_subdiagnostic(sub, self.supports_color()));
        let sub_groups = subs(false).map(|sub| {
            let mut g = Group::with_title(title_from_subdiagnostic(sub, self.supports_color()));
            if let Some(sm) = sm {
                g = g.elements(iter_snippets(sm, &sub.span));
            }
            g
        });

        // Create suggestion groups for non-inline suggestions
        let suggestion_groups = children.iter().flat_map(|suggestion| {
            let sm = self.source_map.as_deref()?;

            // For each substitution, create a separate group
            // Currently we typically only have one substitution per suggestion
            for substitution in &suggestion.substitutions {
                // Group parts by file
                let mut parts_by_file: BTreeMap<_, Vec<_>> = BTreeMap::new();
                for part in &substitution.parts {
                    let file = sm.lookup_source_file(part.span.lo());
                    parts_by_file.entry(file.name.clone()).or_default().push(part);
                }

                if parts_by_file.is_empty() {
                    continue;
                }

                let mut snippets = vec![];
                for (filename, parts) in parts_by_file {
                    let file = sm.get_file_ref(&filename)?;
                    let mut snippet = Snippet::source(file.src.to_string())
                        .path(sm.filename_for_diagnostics(&file.name).to_string())
                        .fold(true);

                    for part in parts {
                        if let Ok(range) = sm.span_to_range(part.span) {
                            snippet = snippet.patch(Patch::new(range, part.snippet.clone()));
                        }
                    }
                    snippets.push(snippet);
                }

                if !snippets.is_empty() {
                    let title = ASLevel::HELP.secondary_title(suggestion.msg.as_str());
                    return Some(Group::with_title(title).elements(snippets));
                }
            }

            None
        });

        let main_group = Group::with_title(title).elements(snippets).elements(footers);
        let report = std::iter::once(main_group)
            .chain(sub_groups)
            .chain(suggestion_groups)
            .collect::<Vec<_>>();
        f(self, &report)
    }
}

/// Diagnostic emitter that emits diagnostics in human-readable format to a local buffer.
pub struct HumanBufferEmitter {
    inner: HumanEmitter,
}

impl Emitter for HumanBufferEmitter {
    #[inline]
    fn emit_diagnostic(&mut self, diagnostic: &mut Diag) {
        self.inner.emit_diagnostic(diagnostic);
    }

    #[inline]
    fn source_map(&self) -> Option<&Arc<SourceMap>> {
        Emitter::source_map(&self.inner)
    }

    #[inline]
    fn supports_color(&self) -> bool {
        self.inner.supports_color()
    }
}

impl HumanBufferEmitter {
    /// Creates a new `BufferEmitter` that writes to a local buffer.
    pub fn new(color_choice: ColorChoice) -> Self {
        Self { inner: HumanEmitter::new(Vec::<u8>::new(), stderr_choice(color_choice)) }
    }

    /// Sets the source map.
    pub fn source_map(mut self, source_map: Option<Arc<SourceMap>>) -> Self {
        self.inner = self.inner.source_map(source_map);
        self
    }

    /// Sets whether to emit diagnostics in a way that is suitable for UI testing.
    pub fn ui_testing(mut self, yes: bool) -> Self {
        self.inner = self.inner.ui_testing(yes);
        self
    }

    /// Returns a reference to the underlying human emitter.
    pub fn inner(&self) -> &HumanEmitter {
        &self.inner
    }

    /// Returns a mutable reference to the underlying human emitter.
    pub fn inner_mut(&mut self) -> &mut HumanEmitter {
        &mut self.inner
    }

    /// Returns a reference to the buffer.
    pub fn buffer(&self) -> &str {
        let buffer = self.inner.downcast_writer::<Vec<u8>>().unwrap();
        debug_assert!(std::str::from_utf8(buffer).is_ok(), "HumanEmitter wrote invalid UTF-8");
        // SAFETY: The buffer is guaranteed to be valid UTF-8.
        unsafe { std::str::from_utf8_unchecked(buffer) }
    }

    /// Returns a mutable reference to the buffer.
    pub fn buffer_mut(&mut self) -> &mut String {
        let buffer = self.inner.downcast_writer_mut::<Vec<u8>>().unwrap();
        debug_assert!(std::str::from_utf8(buffer).is_ok(), "HumanEmitter wrote invalid UTF-8");
        // SAFETY: The buffer is guaranteed to be valid UTF-8.
        unsafe { &mut *(buffer as *mut Vec<u8> as *mut String) }
    }
}

fn title_from_diagnostic(diag: &Diag) -> Title<'_> {
    let mut title = to_as_level(diag.level).primary_title(diag.label());
    if let Some(id) = diag.id() {
        title = title.id(id);
    }
    title
}

fn title_from_subdiagnostic(sub: &SubDiagnostic, supports_color: bool) -> Title<'_> {
    to_as_level(sub.level).secondary_title(sub.label_with_style(supports_color))
}

fn message_from_subdiagnostic(sub: &SubDiagnostic, supports_color: bool) -> Message<'_> {
    to_as_level(sub.level).message(sub.label_with_style(supports_color))
}

fn iter_snippets<'a>(
    sm: &SourceMap,
    msp: &MultiSpan,
) -> impl Iterator<Item = Snippet<'a, Annotation<'a>>> {
    collect_files(sm, msp).into_iter().map(|file| file_to_snippet(sm, &file.file, &file.lines))
}

fn collect_files(sm: &SourceMap, msp: &MultiSpan) -> Vec<FileWithAnnotatedLines> {
    let mut annotated_files = FileWithAnnotatedLines::collect_annotations(sm, msp);
    // Make sure our primary file comes first
    if let Some(primary_span) = msp.primary_span()
        && !primary_span.is_dummy()
        && annotated_files.len() > 1
    {
        let primary_lo = sm.lookup_char_pos(primary_span.lo());
        if let Ok(pos) =
            annotated_files.binary_search_by(|x| x.file.name.cmp(&primary_lo.file.name))
        {
            annotated_files.swap(0, pos);
        }
    }
    annotated_files
}

/// Merges back multi-line annotations that were split across multiple lines into a single
/// annotation that's suitable for `annotate-snippets`.
///
/// Expects that lines are sorted.
fn file_to_snippet<'a>(
    sm: &SourceMap,
    file: &SourceFile,
    lines: &[super::rustc::Line],
) -> Snippet<'a, Annotation<'a>> {
    /// `label, start_idx`
    type MultiLine<'a> = (Option<&'a String>, usize);
    fn multi_line_at<'a, 'b>(
        mls: &'a mut Vec<MultiLine<'b>>,
        depth: usize,
    ) -> &'a mut MultiLine<'b> {
        assert!(depth > 0);
        if mls.len() < depth {
            mls.resize_with(depth, || (None, 0));
        }
        &mut mls[depth - 1]
    }

    debug_assert!(!lines.is_empty());

    let first_line = lines.first().unwrap().line_index;
    debug_assert!(first_line > 0, "line index is 1-based");
    let last_line = lines.last().unwrap().line_index;
    debug_assert!(last_line >= first_line);
    debug_assert!(lines.is_sorted());
    let snippet_base = file.line_position(first_line - 1).unwrap();

    let source = file.get_lines(first_line - 1..=last_line - 1).unwrap_or_default();
    let mut annotations = Vec::new();
    let mut push_annotation = |kind: AnnotationKind, span, label| {
        annotations.push(kind.span(span).label(label));
    };
    let annotation_kind = |is_primary: bool| {
        if is_primary { AnnotationKind::Primary } else { AnnotationKind::Context }
    };

    let mut mls = Vec::new();
    for line in lines {
        let line_abs_pos = file.line_position(line.line_index - 1).unwrap();
        let line_rel_pos = line_abs_pos - snippet_base;
        // Returns the position of the given column in the local snippet.
        // We have to convert the column char position to byte position.
        let rel_pos = |c: &super::rustc::AnnotationColumn| {
            line_rel_pos + char_to_byte_pos(&source[line_rel_pos..], c.file)
        };

        for ann in &line.annotations {
            match ann.annotation_type {
                super::rustc::AnnotationType::Singleline => {
                    push_annotation(
                        annotation_kind(ann.is_primary),
                        rel_pos(&ann.start_col)..rel_pos(&ann.end_col),
                        ann.label.clone().unwrap_or_default(),
                    );
                }
                super::rustc::AnnotationType::MultilineStart(depth) => {
                    *multi_line_at(&mut mls, depth) = (ann.label.as_ref(), rel_pos(&ann.start_col));
                }
                super::rustc::AnnotationType::MultilineLine(_depth) => {
                    // TODO: unvalidated
                    push_annotation(
                        AnnotationKind::Visible,
                        line_rel_pos..line_rel_pos,
                        String::new(),
                    );
                }
                super::rustc::AnnotationType::MultilineEnd(depth) => {
                    let (label, multiline_start_idx) = *multi_line_at(&mut mls, depth);
                    let end_idx = rel_pos(&ann.end_col);
                    debug_assert!(end_idx >= multiline_start_idx);
                    push_annotation(
                        annotation_kind(ann.is_primary),
                        multiline_start_idx..end_idx,
                        label.or(ann.label.as_ref()).cloned().unwrap_or_default(),
                    );
                }
            }
        }
    }
    Snippet::source(source.to_string())
        .path(sm.filename_for_diagnostics(&file.name).to_string())
        .line_start(first_line)
        .fold(true)
        .annotations(annotations)
}

fn to_as_level<'a>(level: Level) -> ASLevel<'a> {
    match level {
        Level::Bug | Level::Fatal | Level::Error | Level::FailureNote => ASLevel::ERROR,
        Level::Warning => ASLevel::WARNING,
        Level::Note | Level::OnceNote => ASLevel::NOTE,
        Level::Help | Level::OnceHelp => ASLevel::HELP,
        Level::Allow => ASLevel::INFO,
    }
    .with_name(if level == Level::FailureNote { None } else { Some(level.to_str()) })
}

fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.chars().take(char_pos).map(char::len_utf8).sum()
}

fn stderr_choice(color_choice: ColorChoice) -> ColorChoice {
    static AUTO: OnceLock<ColorChoice> = OnceLock::new();
    if color_choice == ColorChoice::Auto {
        *AUTO.get_or_init(|| anstream::AutoStream::choice(&std::io::stderr()))
    } else {
        color_choice
    }
}
