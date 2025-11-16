use super::{Diag, Emitter, io_panic, rustc::FileWithAnnotatedLines};
use crate::{
    SourceMap, Span,
    diagnostics::{
        ConfusionType, DiagId, DiagMsg, Level, MultiSpan, SpanLabel, Style, SubDiagnostic,
        SuggestionStyle, Suggestions, detect_confusion_type, emitter::normalize_whitespace,
        is_different,
    },
    pluralize,
    source_map::{FileName, SourceFile},
};
use annotate_snippets::{
    Annotation as ASAnnotation, AnnotationKind, Group, Level as ASLevel, Message, Padding, Patch,
    Renderer, Report, Snippet, Title, renderer::DecorStyle,
};
use anstream::{AutoStream, ColorChoice};
use solar_config::HumanEmitterKind;
use std::{
    any::Any,
    borrow::Cow,
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, OnceLock},
};

type Writer = dyn Write + Send + 'static;

/// Maximum number of suggestions to be shown
///
/// Arbitrary, but taken from trait import suggestion limit
pub(super) const MAX_SUGGESTIONS: usize = 4;

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
    fn emit_diagnostic(&mut self, diag: &mut Diag) {
        let mut primary_span = Cow::Borrowed(&diag.span);
        self.primary_span_formatted(&mut primary_span, &mut diag.suggestions);

        self.emit_messages_default(
            &diag.level,
            &diag.messages,
            &diag.code,
            &primary_span,
            &diag.children,
            &diag.suggestions,
        );

        // self.snippet(diag, |this, snippet| {
        //     writeln!(this.writer, "{}\n", this.renderer.render(snippet))?;
        //     this.writer.flush()
        // })
        // .unwrap_or_else(|e| io_panic(e));
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

    /// Sets the human emitter kind (unicode vs short).
    pub fn human_kind(mut self, kind: HumanEmitterKind) -> Self {
        match kind {
            HumanEmitterKind::Ascii => {
                self.renderer = self.renderer.decor_style(DecorStyle::Ascii);
            }
            HumanEmitterKind::Unicode => {
                self.renderer = self.renderer.decor_style(DecorStyle::Unicode);
            }
            HumanEmitterKind::Short => {
                self.renderer = self.renderer.short_message(true);
            }
            _ => unimplemented!("{kind:?}"),
        }
        self
    }

    /// Sets the terminal width for formatting.
    pub fn terminal_width(mut self, width: Option<usize>) -> Self {
        if let Some(w) = width {
            self.renderer = self.renderer.term_width(w);
        }
        self
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
        let sub_groups = subs(false).map(|sub| {
            let mut g = Group::with_title(title_from_subdiagnostic(sub, self.supports_color()));
            if let Some(sm) = sm {
                g = g.elements(iter_snippets(sm, &sub.span));
            }
            g
        });

        let mut footers =
            subs(true).map(|sub| message_from_subdiagnostic(sub, self.supports_color())).peekable();
        let footer_group =
            footers.peek().is_some().then(|| Group::with_level(ASLevel::NOTE).elements(footers));

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
                            snippet = snippet.patch(Patch::new(range, part.snippet.as_str()));
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

        let main_group = Group::with_title(title).elements(snippets);
        let report = std::iter::once(main_group)
            .chain(suggestion_groups)
            .chain(footer_group)
            .chain(sub_groups)
            .collect::<Vec<_>>();
        f(self, &report)
    }

    fn emit_messages_default(
        &mut self,
        level: &Level,
        msgs: &[(DiagMsg, Style)],
        code: &Option<DiagId>,
        msp: &MultiSpan,
        children: &[SubDiagnostic],
        suggestions: &Suggestions,
    ) {
        let renderer = &self.renderer;
        let annotation_level = annotation_level_for_level(*level);

        // If at least one portion of the message is styled, we need to
        // "pre-style" the message
        let mut title = if msgs.iter().any(|(_, style)| style != &Style::NoStyle) {
            annotation_level.clone().secondary_title(Cow::Owned(self.pre_style_msgs(msgs, *level)))
        } else {
            annotation_level.clone().primary_title(self.no_style_msgs(msgs))
        };

        if let Some(c) = code {
            title = title.id(c.as_string());
            // Unlike rustc, there is no URL associated with DiagId yet.
            // TODO: Add URL mapping
            // if let TerminalUrl::Yes = self.terminal_url {
            //     title = title.id_url(format!("<TODO_URL>/error_codes/{c}.html"));
            // }
        }

        let mut report = vec![];
        let mut main_group = Group::with_title(title);
        let mut footer_group = Group::with_level(ASLevel::NOTE);

        // If we don't have span information, emit and exit
        let Some(sm) = self.source_map.as_ref() else {
            main_group = main_group.elements(children.iter().map(|c| {
                let msg = self.no_style_msgs(&c.messages);
                let level = annotation_level_for_level(c.level);
                level.message(msg)
            }));

            report.push(main_group);
            if let Err(e) = emit_to_destination(renderer.render(&report), level, &mut self.writer) {
                panic!("failed to emit error: {e}");
            }
            return;
        };

        let mut file_ann = collect_annotations(msp, sm);

        // Make sure our primary file comes first
        let primary_span = msp.primary_span().unwrap_or_default();
        if !primary_span.is_dummy() {
            let primary_lo = sm.lookup_char_pos(primary_span.lo());
            if let Ok(pos) = file_ann.binary_search_by(|(f, _)| f.name.cmp(&primary_lo.file.name)) {
                file_ann.swap(0, pos);
            }

            for (file, annotations) in file_ann.into_iter() {
                if let Some(snippet) = self.annotated_snippet(annotations, &file.name, sm) {
                    main_group = main_group.element(snippet);
                }
            }
        }

        for c in children {
            let level = annotation_level_for_level(c.level);

            // If at least one portion of the message is styled, we need to
            // "pre-style" the message
            let msg = if c.messages.iter().any(|(_, style)| style != &Style::NoStyle) {
                Cow::Owned(self.pre_style_msgs(&c.messages, c.level))
            } else {
                Cow::Owned(self.no_style_msgs(&c.messages))
            };

            // This is a secondary message with no span info
            if !c.span.has_primary_spans() && !c.span.has_span_labels() {
                footer_group = footer_group.element(level.clone().message(msg));
                continue;
            }

            report.push(std::mem::replace(
                &mut main_group,
                Group::with_title(level.clone().secondary_title(msg)),
            ));

            let mut file_ann = collect_annotations(&c.span, sm);
            let primary_span = c.span.primary_span().unwrap_or_default();
            if !primary_span.is_dummy() {
                let primary_lo = sm.lookup_char_pos(primary_span.lo());
                if let Ok(pos) =
                    file_ann.binary_search_by(|(f, _)| f.name.cmp(&primary_lo.file.name))
                {
                    file_ann.swap(0, pos);
                }
            }

            for (file, annotations) in file_ann.into_iter() {
                if let Some(snippet) = self.annotated_snippet(annotations, &file.name, sm) {
                    main_group = main_group.element(snippet);
                }
            }
        }

        let suggestions_expected = suggestions
            .iter()
            .filter(|s| {
                matches!(
                    s.style,
                    SuggestionStyle::HideCodeInline
                        | SuggestionStyle::ShowCode
                        | SuggestionStyle::ShowAlways
                )
            })
            .count();
        for suggestion in suggestions.unwrap_tag() {
            match suggestion.style {
                SuggestionStyle::CompletelyHidden => {
                    // do not display this suggestion, it is meant only for tools
                }
                SuggestionStyle::HideCodeAlways => {
                    let msg = self.no_style_msgs(&[(suggestion.msg.to_owned(), Style::HeaderMsg)]);
                    main_group = main_group.element(annotate_snippets::Level::HELP.message(msg));
                }
                SuggestionStyle::HideCodeInline
                | SuggestionStyle::ShowCode
                | SuggestionStyle::ShowAlways => {
                    let substitutions = suggestion
                        .substitutions
                        .iter()
                        .cloned() // clone is required to sort and filter duplicated spans
                        .filter_map(|mut subst| {
                            // Suggestions coming from macros can have malformed spans. This is a
                            // heavy handed approach to avoid ICEs by
                            // ignoring the suggestion outright.
                            let invalid =
                                subst.parts.iter().any(|item| sm.is_valid_span(item.span).is_err());
                            if invalid {
                                debug!("suggestion contains an invalid span: {:?}", subst);
                            }

                            // Assumption: all spans are in the same file, and all spans
                            // are disjoint. Sort in ascending order.
                            subst.parts.sort_by_key(|part| part.span.lo());
                            // Verify the assumption that all spans are disjoint
                            assert_eq!(
                                subst.parts.windows(2).find(|s| s[0].span.overlaps(s[1].span)),
                                None,
                                "all spans must be disjoint",
                            );

                            // Account for cases where we are suggesting the same code that's
                            // already there. This shouldn't happen
                            // often, but in some cases for multipart
                            // suggestions it's much easier to handle it here than in the origin.
                            subst.parts.retain(|p| is_different(sm, &p.snippet, p.span));

                            if !invalid { Some(subst) } else { None }
                        })
                        .collect::<Vec<_>>();

                    if substitutions.is_empty() {
                        continue;
                    }
                    let mut msg = suggestion.msg.to_string();

                    let lo = substitutions
                        .iter()
                        .find_map(|sub| sub.parts.first().map(|p| p.span.lo()))
                        .unwrap();
                    let file = sm.lookup_source_file(lo);

                    let filename = sm.filename_for_diagnostics(&file.name).to_string();

                    let other_suggestions = substitutions.len().saturating_sub(MAX_SUGGESTIONS);

                    let subs = substitutions
                        .into_iter()
                        .take(MAX_SUGGESTIONS)
                        .filter_map(|sub| {
                            let mut confusion_type = ConfusionType::None;
                            for part in &sub.parts {
                                let part_confusion =
                                    detect_confusion_type(sm, &part.snippet, part.span);
                                confusion_type = confusion_type.combine(part_confusion);
                            }

                            if !matches!(confusion_type, ConfusionType::None) {
                                msg.push_str(confusion_type.label_text());
                            }

                            let parts = sub
                                .parts
                                .into_iter()
                                .filter_map(|p| {
                                    if is_different(sm, &p.snippet, p.span) {
                                        Some((p.span, p.snippet))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>();

                            if parts.is_empty() {
                                None
                            } else {
                                let spans = parts.iter().map(|(span, _)| *span).collect::<Vec<_>>();

                                // Unlike rustc, there is no attribute suggestion in Solidity (yet).
                                // When similar feature to attribute arrives, refer to rustc's
                                // implementation https://github.com/rust-lang/rust/blob/4146079cee94242771864147e32fb5d9adbd34f8/compiler/rustc_errors/src/annotate_snippet_emitter_writer.rs#L424
                                let fold = true;

                                if let Some((bounding_span, source, line_offset)) =
                                    shrink_file(spans.as_slice(), &file.name, sm)
                                {
                                    let adj_lo = bounding_span.lo().to_usize();
                                    Some(
                                        Snippet::source(source)
                                            .line_start(line_offset)
                                            .path(filename.clone())
                                            .fold(fold)
                                            .patches(parts.into_iter().map(
                                                |(span, replacement)| {
                                                    let lo =
                                                        span.lo().to_usize().saturating_sub(adj_lo);
                                                    let hi =
                                                        span.hi().to_usize().saturating_sub(adj_lo);

                                                    Patch::new(lo..hi, replacement.into_inner())
                                                },
                                            )),
                                    )
                                } else {
                                    None
                                }
                            }
                        })
                        .collect::<Vec<_>>();
                    if !subs.is_empty() {
                        report.push(std::mem::replace(
                            &mut main_group,
                            Group::with_title(annotate_snippets::Level::HELP.secondary_title(msg)),
                        ));

                        main_group = main_group.elements(subs);
                        if other_suggestions > 0 {
                            main_group = main_group.element(
                                annotate_snippets::Level::NOTE.no_name().message(format!(
                                    "and {} other candidate{}",
                                    other_suggestions,
                                    pluralize!(other_suggestions)
                                )),
                            );
                        }
                    }
                }
            }
        }

        // FIXME: This hack should be removed once annotate_snippets is the
        // default emitter.
        if suggestions_expected > 0 && report.is_empty() {
            main_group = main_group.element(Padding);
        }

        if !main_group.is_empty() {
            report.push(main_group);
        }

        if !footer_group.is_empty() {
            report.push(footer_group);
        }

        if let Err(e) = emit_to_destination(renderer.render(&report), level, &mut self.writer) {
            panic!("failed to emit error: {e}");
        }
    }

    fn pre_style_msgs(&self, msgs: &[(DiagMsg, Style)], level: Level) -> String {
        msgs.iter()
            .filter_map(|(m, style)| {
                let text = m.as_str();
                let style = style.to_color_spec(level);
                if text.is_empty() { None } else { Some(format!("{style}{text}{style:#}")) }
            })
            .collect()
    }

    // Unlike rustc, there is no translation.
    // Since the behavior of `translator.translate_messages` does not contains styling,
    // this function to concatenate messages instead can be used.
    fn no_style_msgs(&self, msgs: &[(DiagMsg, Style)]) -> String {
        msgs.iter().map(|(m, _)| m.as_str()).collect()
    }

    fn annotated_snippet<'a>(
        &self,
        annotations: Vec<Annotation>,
        file_name: &FileName,
        sm: &Arc<SourceMap>,
    ) -> Option<Snippet<'a, ASAnnotation<'a>>> {
        let spans = annotations.iter().map(|a| a.span).collect::<Vec<_>>();
        if let Some((bounding_span, source, offset_line)) = shrink_file(&spans, file_name, sm) {
            let adj_lo = bounding_span.lo().to_usize();

            let filename = sm.filename_for_diagnostics(file_name).to_string();

            Some(Snippet::source(source).line_start(offset_line).path(filename).annotations(
                annotations.into_iter().map(move |a| {
                    let lo = a.span.lo().to_usize().saturating_sub(adj_lo);
                    let hi = a.span.hi().to_usize().saturating_sub(adj_lo);
                    let ann = a.kind.span(lo..hi);
                    if let Some(label) = a.label { ann.label(label) } else { ann }
                }),
            ))
        } else {
            None
        }
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

    /// Sets the human emitter kind (unicode vs short).
    pub fn human_kind(mut self, kind: HumanEmitterKind) -> Self {
        self.inner = self.inner.human_kind(kind);
        self
    }

    /// Sets the terminal width for formatting.
    pub fn terminal_width(mut self, width: Option<usize>) -> Self {
        self.inner = self.inner.terminal_width(width);
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
    let mut title = annotation_level_for_level(diag.level).primary_title(diag.label());
    if let Some(id) = diag.id() {
        title = title.id(id);
    }
    title
}

fn title_from_subdiagnostic(sub: &SubDiagnostic, supports_color: bool) -> Title<'_> {
    annotation_level_for_level(sub.level).secondary_title(sub.label_with_style(supports_color))
}

fn message_from_subdiagnostic(sub: &SubDiagnostic, supports_color: bool) -> Message<'_> {
    annotation_level_for_level(sub.level).message(sub.label_with_style(supports_color))
}

fn iter_snippets<'a>(
    sm: &SourceMap,
    msp: &MultiSpan,
) -> impl Iterator<Item = Snippet<'a, ASAnnotation<'a>>> {
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
) -> Snippet<'a, ASAnnotation<'a>> {
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

fn annotation_level_for_level<'a>(level: Level) -> ASLevel<'a> {
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

fn emit_to_destination(rendered: String, lvl: &Level, dst: &mut Writer) -> io::Result<()> {
    // Unlike rustc, there is no lock
    // use crate::lock;
    // let _buffer_lock = lock::acquire_global_lock("rustc_errors");

    writeln!(dst, "{rendered}")?;
    if !lvl.is_failure_note() {
        writeln!(dst)?;
    }
    dst.flush()?;
    Ok(())
}

#[derive(Debug)]
struct Annotation {
    kind: AnnotationKind,
    span: Span,
    label: Option<String>,
}

fn collect_annotations(
    msp: &MultiSpan,
    sm: &Arc<SourceMap>,
) -> Vec<(Arc<SourceFile>, Vec<Annotation>)> {
    let mut output: Vec<(Arc<SourceFile>, Vec<Annotation>)> = vec![];

    for SpanLabel { span, is_primary, label } in msp.span_labels() {
        // If we don't have a useful span, pick the primary span if that exists.
        // Worst case we'll just print an error at the top of the main file.
        let span = match (span.is_dummy(), msp.primary_span()) {
            (_, None) | (false, _) => span,
            (true, Some(span)) => span,
        };
        let file = sm.lookup_source_file(span.lo());

        let kind = if is_primary { AnnotationKind::Primary } else { AnnotationKind::Context };

        let label = label.as_ref().map(|m| normalize_whitespace(m));

        let ann = Annotation { kind, span, label };
        if sm.is_valid_span(ann.span).is_ok() {
            if let Some((_, annotations)) = output.iter_mut().find(|(f, _)| f.name == file.name) {
                annotations.push(ann);
            } else {
                output.push((file, vec![ann]));
            }
        }
    }
    output
}

fn shrink_file(
    spans: &[Span],
    _file_name: &FileName,
    sm: &Arc<SourceMap>,
) -> Option<(Span, String, usize)> {
    let lo_byte = spans.iter().map(|s| s.lo()).min()?;
    let lo_loc = sm.lookup_char_pos(lo_byte);
    let lo = lo_loc.file.line_bounds(lo_loc.line.saturating_sub(1)).start;

    let hi_byte = spans.iter().map(|s| s.hi()).max()?;
    let hi_loc = sm.lookup_char_pos(hi_byte);
    let hi = lo_loc.file.line_bounds(hi_loc.line.saturating_sub(1)).end;

    let bounding_span = Span::new(lo, hi);
    let source = sm.span_to_snippet(bounding_span).unwrap_or_default();
    let offset_line = lo_loc.line;

    Some((bounding_span, source, offset_line))
}
