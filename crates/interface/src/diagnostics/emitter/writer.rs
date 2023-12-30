use super::Diagnostic;
use crate::{
    diagnostics::{ColorConfig, Level, Style},
    SourceMap,
};
use annotate_snippets::{Annotation, AnnotationType, Renderer, Snippet};
use anstream::{AutoStream, ColorChoice};
use std::io::{self, Write};
use sulk_data_structures::sync::Lrc;

const fn make_renderer(anonymize: bool) -> Renderer {
    Renderer::plain()
        .anonymized_line_numbers(anonymize)
        .error(Level::Error.style())
        .warning(Level::Warning.style())
        .info(Level::Note.style())
        .note(Level::Note.style())
        .help(Level::Help.style())
        .line_no(Style::LineNumber.to_color_spec(Level::Note))
        .emphasis(anstyle::Style::new().bold())
        .none(anstyle::Style::new())
}

static DEFAULT_RENDERER: Renderer = make_renderer(false);
static ANON_RENDERER: Renderer = make_renderer(true);

/// Diagnostic emitter that emits to an arbitrary [`std::io::Write`] writer.
pub struct EmitterWriter {
    writer: AutoStream<Box<dyn io::Write>>,
    source_map: Option<Lrc<SourceMap>>,
    renderer: &'static Renderer,
}

impl crate::diagnostics::Emitter for EmitterWriter {
    fn emit_diagnostic(&mut self, diagnostic: &Diagnostic) {
        self.snippet(diagnostic, |this, snippet| {
            if let Err(e) = write!(this.writer, "{}", this.renderer.render(snippet)) {
                panic!("failed to emit diagnostic: {e}");
            }
        });
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

impl EmitterWriter {
    /// Creates a new `EmitterWriter` that writes to given writer.
    pub fn new(writer: Box<dyn io::Write>, color: ColorConfig) -> Self {
        let writer = AutoStream::new(writer, color.to_color_choice());
        Self { writer, source_map: None, renderer: &DEFAULT_RENDERER }
    }

    /// Creates a new `EmitterWriter` that writes to stderr.
    pub fn stderr(color_choice: ColorConfig) -> Self {
        Self::new(Box::new(io::stderr()), color_choice)
    }

    /// Sets the source map.
    pub fn source_map(mut self, source_map: Option<Lrc<SourceMap>>) -> Self {
        self.source_map = source_map;
        self
    }

    /// Sets whether to anonymize line numbers.
    pub fn anonymized_line_numbers(mut self, anonymized_line_numbers: bool) -> Self {
        self.renderer = if anonymized_line_numbers { &DEFAULT_RENDERER } else { &ANON_RENDERER };
        self
    }

    /// Formats the given `diagnostic` into a [`Snippet`] suitable for use with the renderer.
    fn snippet(&mut self, diagnostic: &Diagnostic, f: impl FnOnce(&mut Self, Snippet<'_>)) {
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

        // Dummy subdiagnostics go in the footer.
        // We have to allocate here for the labels.
        let (dummy_subs, subs) = diagnostic
            .children
            .iter()
            .map(|sub| (sub.label(), sub))
            .partition::<Vec<_>, _>(|(_, s)| s.span.is_dummy());

        // TODO: `FileWithAnnotatedLines::collect_annotations` and then make `Slice`s.
        // https://github.com/rust-lang/rust/blob/824667f75357bb394c55ef3b0e2095af62e68a19/compiler/rustc_errors/src/annotate_snippet_emitter_writer.rs#L138

        // TODO: Add `:{line}:{col}` to `origin`, since it doesn't look like `annotate-snippets`
        // does it.

        let _ = subs;

        let label = diagnostic.label();
        let snippet = Snippet {
            title: Some(Annotation {
                label: Some(&label),
                id: diagnostic.code.as_ref().map(|s| s.id),
                annotation_type: to_annotation_type(diagnostic.level),
            }),
            slices: vec![],
            footer: dummy_subs
                .iter()
                .map(|(label, sub)| Annotation {
                    label: Some(label),
                    id: None,
                    annotation_type: to_annotation_type(sub.level),
                })
                .collect(),
        };
        f(self, snippet);
    }
}

fn to_annotation_type(level: Level) -> AnnotationType {
    match level {
        Level::Fatal | Level::Error => AnnotationType::Error,
        Level::Warning => AnnotationType::Warning,
        Level::Note | Level::OnceNote | Level::FailureNote => AnnotationType::Note,
        Level::Help | Level::OnceHelp => AnnotationType::Help,
        Level::Allow => AnnotationType::Info,
    }
}
