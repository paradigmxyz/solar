use super::{io_panic, rustc::FileWithAnnotatedLines, Diagnostic, Emitter};
use crate::{
    diagnostics::{Level, MultiSpan, Style, SubDiagnostic},
    source_map::SourceFile,
    SourceMap,
};
use annotate_snippets::{Annotation, Level as ASLevel, Message, Renderer, Snippet};
use anstream::{AutoStream, ColorChoice};
use std::{
    any::Any,
    io::{self, Write},
    ops::Range,
    sync::Arc,
};

// TODO: Tabs are not formatted correctly: https://github.com/rust-lang/annotate-snippets-rs/issues/25

type Writer = dyn Write + Send + 'static;

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
    writer_type_id: std::any::TypeId,
    real_writer: *mut Writer,
    writer: AutoStream<Box<Writer>>,
    source_map: Option<Arc<SourceMap>>,
    renderer: Renderer,
}

// SAFETY: `real_writer` always points to the `Writer` in `writer`.
unsafe impl Send for HumanEmitter {}

impl Emitter for HumanEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
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
        // TODO: Clean this up on next anstream release
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

        Self::new(TestWriter(io::stderr()), ColorChoice::Always)
    }

    /// Creates a new `HumanEmitter` that writes to stderr.
    pub fn stderr(mut color_choice: ColorChoice) -> Self {
        let stderr = io::stderr();
        // Call `AutoStream::choice` on `io::Stderr` rather than later on `Box<dyn Write>`.
        if color_choice == ColorChoice::Auto {
            color_choice = AutoStream::choice(&stderr);
        }
        Self::new(io::BufWriter::new(stderr), color_choice)
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
        diagnostic: &Diagnostic,
        f: impl FnOnce(&mut Self, Message<'_>) -> R,
    ) -> R {
        // Current format (annotate-snippets 0.10.0) (comments in <...>):
        /*
        title.level[title.id]: title.label
           --> snippets[0].origin
            |
         LL | snippets[0].source[ann[0].range] <ann = snippets[0].annotations>
            | ^^^^^^^^^^^^^^^^ ann[0].level: ann[0].label <type is skipped for error, warning>
         LL | snippets[0].source[ann[1].range]
            | ---------------- ann[1].level: ann[1].label
            |
           ::: snippets[1].origin
            |
        etc...
            |
            = footer[0].level: footer[0].label <I believe the .id here is always ignored>
            = footer[1].level: footer[1].label
            = ...
        */

        let title = OwnedMessage::from_diagnostic(diagnostic);

        let owned_snippets = self
            .source_map
            .as_deref()
            .map(|sm| OwnedSnippet::collect(sm, diagnostic))
            .unwrap_or_default();

        // Dummy subdiagnostics go in the footer, while non-dummy ones go in the slices.
        let owned_footers: Vec<_> = diagnostic
            .children
            .iter()
            .filter(|sub| sub.span.is_dummy())
            .map(OwnedMessage::from_subdiagnostic)
            .collect();

        let snippet = title
            .as_ref()
            .snippets(owned_snippets.iter().map(OwnedSnippet::as_ref))
            .footers(owned_footers.iter().map(OwnedMessage::as_ref));
        f(self, snippet)
    }
}

/// Diagnostic emitter that emits diagnostics in human-readable format to a local buffer.
pub struct HumanBufferEmitter {
    inner: HumanEmitter,
}

impl Emitter for HumanBufferEmitter {
    #[inline]
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
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
    pub fn new(mut color: ColorChoice) -> Self {
        if color == ColorChoice::Auto {
            color = anstream::AutoStream::choice(&std::io::stderr());
        }
        Self { inner: HumanEmitter::new(Vec::<u8>::new(), color) }
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

#[derive(Debug)]
struct OwnedMessage {
    id: Option<String>,
    label: String,
    level: ASLevel,
}

impl OwnedMessage {
    fn from_diagnostic(diag: &Diagnostic) -> Self {
        Self { id: diag.id(), label: diag.label().into_owned(), level: to_as_level(diag.level) }
    }

    fn from_subdiagnostic(sub: &SubDiagnostic) -> Self {
        Self { id: None, label: sub.label().into_owned(), level: to_as_level(sub.level) }
    }

    fn as_ref(&self) -> Message<'_> {
        let mut msg = self.level.title(&self.label);
        if let Some(id) = &self.id {
            msg = msg.id(id);
        }
        msg
    }
}

#[derive(Debug)]
struct OwnedAnnotation {
    range: Range<usize>,
    label: String,
    level: ASLevel,
}

impl OwnedAnnotation {
    fn as_ref(&self) -> Annotation<'_> {
        self.level.span(self.range.clone()).label(&self.label)
    }
}

#[derive(Debug)]
struct OwnedSnippet {
    origin: String,
    source: String,
    line_start: usize,
    fold: bool,
    annotations: Vec<OwnedAnnotation>,
}

impl OwnedSnippet {
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
                        if ann.is_primary && ann.label.is_none() {
                            ann.label = Some(label.to_string());
                        }
                    }
                }

                if let Some(main_file) =
                    files.iter_mut().find(|main_file| Arc::ptr_eq(&main_file.file, &sub_file.file))
                {
                    main_file.add_lines(sub_file.lines);
                } else {
                    files.push(sub_file);
                }
            }
        }

        files
            .iter()
            .map(|file| file_to_snippet(sm, &file.file, &file.lines, diagnostic.level))
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

    fn as_ref(&self) -> Snippet<'_> {
        Snippet::source(&self.source)
            .line_start(self.line_start)
            .origin(&self.origin)
            .fold(self.fold)
            .annotations(self.annotations.iter().map(OwnedAnnotation::as_ref))
    }
}

/// Merges back multi-line annotations that were split across multiple lines into a single
/// annotation that's suitable for `annotate-snippets`.
///
/// Expects that lines are sorted.
fn file_to_snippet(
    sm: &SourceMap,
    file: &SourceFile,
    lines: &[super::rustc::Line],
    default_level: Level,
) -> OwnedSnippet {
    debug_assert!(!lines.is_empty());

    let first_line = lines.first().unwrap().line_index;
    debug_assert!(first_line > 0, "line index is 1-based");
    let last_line = lines.last().unwrap().line_index;
    debug_assert!(last_line >= first_line);
    debug_assert!(lines.is_sorted());
    let snippet_base = file.line_position(first_line - 1).unwrap();

    let mut snippet = OwnedSnippet {
        origin: sm.filename_for_diagnostics(&file.name).to_string(),
        source: file.get_lines(first_line - 1..=last_line - 1).unwrap_or_default().into(),
        line_start: first_line,
        fold: true,
        annotations: Vec::new(),
    };
    let mut multiline_start = None;
    for line in lines {
        let line_abs_pos = file.line_position(line.line_index - 1).unwrap();
        let line_rel_pos = line_abs_pos - snippet_base;
        // Returns the position of the given column in the local snippet.
        // We have to convert the column char position to byte position.
        let rel_pos = |c: &super::rustc::AnnotationColumn| {
            line_rel_pos + char_to_byte_pos(&snippet.source[line_rel_pos..], c.file)
        };

        for ann in &line.annotations {
            match ann.annotation_type {
                super::rustc::AnnotationType::Singleline => {
                    snippet.annotations.push(OwnedAnnotation {
                        range: rel_pos(&ann.start_col)..rel_pos(&ann.end_col),
                        label: ann.label.clone().unwrap_or_default(),
                        level: to_as_level(ann.level.unwrap_or(default_level)),
                    });
                }
                super::rustc::AnnotationType::MultilineStart(_) => {
                    debug_assert!(multiline_start.is_none());
                    multiline_start = Some((ann.label.as_ref(), rel_pos(&ann.start_col)));
                }
                super::rustc::AnnotationType::MultilineLine(_) => {}
                super::rustc::AnnotationType::MultilineEnd(_) => {
                    let (label, multiline_start_idx) = multiline_start.take().unwrap();
                    let end_idx = rel_pos(&ann.end_col);
                    debug_assert!(end_idx >= multiline_start_idx);
                    snippet.annotations.push(OwnedAnnotation {
                        range: multiline_start_idx..end_idx,
                        label: label.or(ann.label.as_ref()).cloned().unwrap_or_default(),
                        level: to_as_level(ann.level.unwrap_or(default_level)),
                    });
                }
            }
        }
    }
    snippet
}

fn to_as_level(level: Level) -> ASLevel {
    match level {
        Level::Bug | Level::Fatal | Level::Error => ASLevel::Error,
        Level::Warning => ASLevel::Warning,
        Level::Note | Level::OnceNote | Level::FailureNote => ASLevel::Note,
        Level::Help | Level::OnceHelp => ASLevel::Help,
        Level::Allow => ASLevel::Info,
    }
}

fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.chars().take(char_pos).map(char::len_utf8).sum()
}
