use rsolc_span::Span;

// #[derive(Subdiagnostic)]
pub enum TokenSubstitution {
    // #[suggestion(parse_sugg_quotes, code = "{suggestion}", applicability = "maybe-incorrect")]
    DirectedQuotes {
        // #[primary_span]
        span: Span,
        suggestion: String,
        ascii_str: &'static str,
        ascii_name: &'static str,
    },
    // #[suggestion(parse_sugg_other, code = "{suggestion}", applicability = "maybe-incorrect")]
    Other {
        // #[primary_span]
        span: Span,
        suggestion: String,
        ch: String,
        u_name: &'static str,
        ascii_str: &'static str,
        ascii_name: &'static str,
    },
}
