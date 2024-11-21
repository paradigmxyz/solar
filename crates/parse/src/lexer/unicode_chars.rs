//! Characters and their corresponding confusables were collected from
//! <https://www.unicode.org/Public/security/10.0.0/confusables.txt>

#![allow(unused_imports, dead_code)]

use super::Lexer;
use solar_ast::token::{BinOpToken, Delimiter, TokenKind};
use solar_interface::{sym, BytePos, Span};

#[rustfmt::skip] // for line breaks
pub(crate) const UNICODE_ARRAY: &[(char, &str, &str)] = &[
    (' ', "Line Separator", " "),
    (' ', "Paragraph Separator", " "),
    (' ', "Ogham Space mark", " "),
    (' ', "En Quad", " "),
    (' ', "Em Quad", " "),
    (' ', "En Space", " "),
    (' ', "Em Space", " "),
    (' ', "Three-Per-Em Space", " "),
    (' ', "Four-Per-Em Space", " "),
    (' ', "Six-Per-Em Space", " "),
    (' ', "Punctuation Space", " "),
    (' ', "Thin Space", " "),
    (' ', "Hair Space", " "),
    (' ', "Medium Mathematical Space", " "),
    (' ', "No-Break Space", " "),
    (' ', "Figure Space", " "),
    (' ', "Narrow No-Break Space", " "),
    ('　', "Ideographic Space", " "),

    ('ߺ', "Nko Lajanyalan", "_"),
    ('﹍', "Dashed Low Line", "_"),
    ('﹎', "Centreline Low Line", "_"),
    ('﹏', "Wavy Low Line", "_"),
    ('＿', "Fullwidth Low Line", "_"),

    ('‐', "Hyphen", "-"),
    ('‑', "Non-Breaking Hyphen", "-"),
    ('‒', "Figure Dash", "-"),
    ('–', "En Dash", "-"),
    ('—', "Em Dash", "-"),
    ('﹘', "Small Em Dash", "-"),
    ('۔', "Arabic Full Stop", "-"),
    ('⁃', "Hyphen Bullet", "-"),
    ('˗', "Modifier Letter Minus Sign", "-"),
    ('−', "Minus Sign", "-"),
    ('➖', "Heavy Minus Sign", "-"),
    ('Ⲻ', "Coptic Letter Dialect-P Ni", "-"),
    ('ー', "Katakana-Hiragana Prolonged Sound Mark", "-"),
    ('－', "Fullwidth Hyphen-Minus", "-"),
    ('―', "Horizontal Bar", "-"),
    ('─', "Box Drawings Light Horizontal", "-"),
    ('━', "Box Drawings Heavy Horizontal", "-"),
    ('㇐', "CJK Stroke H", "-"),
    ('ꟷ', "Latin Epigraphic Letter Sideways I", "-"),
    ('ᅳ', "Hangul Jungseong Eu", "-"),
    ('ㅡ', "Hangul Letter Eu", "-"),
    ('一', "CJK Unified Ideograph-4E00", "-"),
    ('⼀', "Kangxi Radical One", "-"),

    ('؍', "Arabic Date Separator", ","),
    ('٫', "Arabic Decimal Separator", ","),
    ('‚', "Single Low-9 Quotation Mark", ","),
    ('¸', "Cedilla", ","),
    ('ꓹ', "Lisu Letter Tone Na Po", ","),
    ('，', "Fullwidth Comma", ","),

    (';', "Greek Question Mark", ";"),
    ('；', "Fullwidth Semicolon", ";"),
    ('︔', "Presentation Form For Vertical Semicolon", ";"),

    ('ः', "Devanagari Sign Visarga", ":"),
    ('ઃ', "Gujarati Sign Visarga", ":"),
    ('：', "Fullwidth Colon", ":"),
    ('։', "Armenian Full Stop", ":"),
    ('܃', "Syriac Supralinear Colon", ":"),
    ('܄', "Syriac Sublinear Colon", ":"),
    ('᛬', "Runic Multiple Punctuation", ":"),
    ('︰', "Presentation Form For Vertical Two Dot Leader", ":"),
    ('᠃', "Mongolian Full Stop", ":"),
    ('᠉', "Mongolian Manchu Full Stop", ":"),
    ('⁚', "Two Dot Punctuation", ":"),
    ('׃', "Hebrew Punctuation Sof Pasuq", ":"),
    ('˸', "Modifier Letter Raised Colon", ":"),
    ('꞉', "Modifier Letter Colon", ":"),
    ('∶', "Ratio", ":"),
    ('ː', "Modifier Letter Triangular Colon", ":"),
    ('ꓽ', "Lisu Letter Tone Mya Jeu", ":"),
    ('︓', "Presentation Form For Vertical Colon", ":"),

    ('！', "Fullwidth Exclamation Mark", "!"),
    ('ǃ', "Latin Letter Retroflex Click", "!"),
    ('ⵑ', "Tifinagh Letter Tuareg Yang", "!"),
    ('︕', "Presentation Form For Vertical Exclamation Mark", "!"),

    ('ʔ', "Latin Letter Glottal Stop", "?"),
    ('Ɂ', "Latin Capital Letter Glottal Stop", "?"),
    ('ॽ', "Devanagari Letter Glottal Stop", "?"),
    ('Ꭾ', "Cherokee Letter He", "?"),
    ('ꛫ', "Bamum Letter Ntuu", "?"),
    ('？', "Fullwidth Question Mark", "?"),
    ('︖', "Presentation Form For Vertical Question Mark", "?"),

    ('𝅭', "Musical Symbol Combining Augmentation Dot", "."),
    ('․', "One Dot Leader", "."),
    ('܁', "Syriac Supralinear Full Stop", "."),
    ('܂', "Syriac Sublinear Full Stop", "."),
    ('꘎', "Vai Full Stop", "."),
    ('𐩐', "Kharoshthi Punctuation Dot", "."),
    ('٠', "Arabic-Indic Digit Zero", "."),
    ('۰', "Extended Arabic-Indic Digit Zero", "."),
    ('ꓸ', "Lisu Letter Tone Mya Ti", "."),
    ('·', "Middle Dot", "."),
    ('・', "Katakana Middle Dot", "."),
    ('･', "Halfwidth Katakana Middle Dot", "."),
    ('᛫', "Runic Single Punctuation", "."),
    ('·', "Greek Ano Teleia", "."),
    ('⸱', "Word Separator Middle Dot", "."),
    ('𐄁', "Aegean Word Separator Dot", "."),
    ('•', "Bullet", "."),
    ('‧', "Hyphenation Point", "."),
    ('∙', "Bullet Operator", "."),
    ('⋅', "Dot Operator", "."),
    ('ꞏ', "Latin Letter Sinological Dot", "."),
    ('ᐧ', "Canadian Syllabics Final Middle Dot", "."),
    ('ᐧ', "Canadian Syllabics Final Middle Dot", "."),
    ('．', "Fullwidth Full Stop", "."),
    ('。', "Ideographic Full Stop", "."),
    ('︒', "Presentation Form For Vertical Ideographic Full Stop", "."),

    ('՝', "Armenian Comma", "\'"),
    ('＇', "Fullwidth Apostrophe", "\'"),
    ('‘', "Left Single Quotation Mark", "\'"),
    ('’', "Right Single Quotation Mark", "\'"),
    ('‛', "Single High-Reversed-9 Quotation Mark", "\'"),
    ('′', "Prime", "\'"),
    ('‵', "Reversed Prime", "\'"),
    ('՚', "Armenian Apostrophe", "\'"),
    ('׳', "Hebrew Punctuation Geresh", "\'"),
    ('`', "Grave Accent", "\'"),
    ('`', "Greek Varia", "\'"),
    ('｀', "Fullwidth Grave Accent", "\'"),
    ('´', "Acute Accent", "\'"),
    ('΄', "Greek Tonos", "\'"),
    ('´', "Greek Oxia", "\'"),
    ('᾽', "Greek Koronis", "\'"),
    ('᾿', "Greek Psili", "\'"),
    ('῾', "Greek Dasia", "\'"),
    ('ʹ', "Modifier Letter Prime", "\'"),
    ('ʹ', "Greek Numeral Sign", "\'"),
    ('ˈ', "Modifier Letter Vertical Line", "\'"),
    ('ˊ', "Modifier Letter Acute Accent", "\'"),
    ('ˋ', "Modifier Letter Grave Accent", "\'"),
    ('˴', "Modifier Letter Middle Grave Accent", "\'"),
    ('ʻ', "Modifier Letter Turned Comma", "\'"),
    ('ʽ', "Modifier Letter Reversed Comma", "\'"),
    ('ʼ', "Modifier Letter Apostrophe", "\'"),
    ('ʾ', "Modifier Letter Right Half Ring", "\'"),
    ('ꞌ', "Latin Small Letter Saltillo", "\'"),
    ('י', "Hebrew Letter Yod", "\'"),
    ('ߴ', "Nko High Tone Apostrophe", "\'"),
    ('ߵ', "Nko Low Tone Apostrophe", "\'"),
    ('ᑊ', "Canadian Syllabics West-Cree P", "\'"),
    ('ᛌ', "Runic Letter Short-Twig-Sol S", "\'"),
    ('𖽑', "Miao Sign Aspiration", "\'"),
    ('𖽒', "Miao Sign Reformed Voicing", "\'"),

    ('᳓', "Vedic Sign Nihshvasa", "\""),
    ('＂', "Fullwidth Quotation Mark", "\""),
    ('“', "Left Double Quotation Mark", "\""),
    ('”', "Right Double Quotation Mark", "\""),
    ('‟', "Double High-Reversed-9 Quotation Mark", "\""),
    ('″', "Double Prime", "\""),
    ('‶', "Reversed Double Prime", "\""),
    ('〃', "Ditto Mark", "\""),
    ('״', "Hebrew Punctuation Gershayim", "\""),
    ('˝', "Double Acute Accent", "\""),
    ('ʺ', "Modifier Letter Double Prime", "\""),
    ('˶', "Modifier Letter Middle Double Acute Accent", "\""),
    ('˵', "Modifier Letter Middle Double Grave Accent", "\""),
    ('ˮ', "Modifier Letter Double Apostrophe", "\""),
    ('ײ', "Hebrew Ligature Yiddish Double Yod", "\""),
    ('❞', "Heavy Double Comma Quotation Mark Ornament", "\""),
    ('❝', "Heavy Double Turned Comma Quotation Mark Ornament", "\""),

    ('（', "Fullwidth Left Parenthesis", "("),
    ('❨', "Medium Left Parenthesis Ornament", "("),
    ('﴾', "Ornate Left Parenthesis", "("),

    ('）', "Fullwidth Right Parenthesis", ")"),
    ('❩', "Medium Right Parenthesis Ornament", ")"),
    ('﴿', "Ornate Right Parenthesis", ")"),

    ('［', "Fullwidth Left Square Bracket", "["),
    ('❲', "Light Left Tortoise Shell Bracket Ornament", "["),
    ('「', "Left Corner Bracket", "["),
    ('『', "Left White Corner Bracket", "["),
    ('【', "Left Black Lenticular Bracket", "["),
    ('〔', "Left Tortoise Shell Bracket", "["),
    ('〖', "Left White Lenticular Bracket", "["),
    ('〘', "Left White Tortoise Shell Bracket", "["),
    ('〚', "Left White Square Bracket", "["),

    ('］', "Fullwidth Right Square Bracket", "]"),
    ('❳', "Light Right Tortoise Shell Bracket Ornament", "]"),
    ('」', "Right Corner Bracket", "]"),
    ('』', "Right White Corner Bracket", "]"),
    ('】', "Right Black Lenticular Bracket", "]"),
    ('〕', "Right Tortoise Shell Bracket", "]"),
    ('〗', "Right White Lenticular Bracket", "]"),
    ('〙', "Right White Tortoise Shell Bracket", "]"),
    ('〛', "Right White Square Bracket", "]"),

    ('❴', "Medium Left Curly Bracket Ornament", "{"),
    ('𝄔', "Musical Symbol Brace", "{"),
    ('｛', "Fullwidth Left Curly Bracket", "{"),

    ('❵', "Medium Right Curly Bracket Ornament", "}"),
    ('｝', "Fullwidth Right Curly Bracket", "}"),

    ('⁎', "Low Asterisk", "*"),
    ('٭', "Arabic Five Pointed Star", "*"),
    ('∗', "Asterisk Operator", "*"),
    ('𐌟', "Old Italic Letter Ess", "*"),
    ('＊', "Fullwidth Asterisk", "*"),

    ('᜵', "Philippine Single Punctuation", "/"),
    ('⁁', "Caret Insertion Point", "/"),
    ('∕', "Division Slash", "/"),
    ('⁄', "Fraction Slash", "/"),
    ('╱', "Box Drawings Light Diagonal Upper Right To Lower Left", "/"),
    ('⟋', "Mathematical Rising Diagonal", "/"),
    ('⧸', "Big Solidus", "/"),
    ('𝈺', "Greek Instrumental Notation Symbol-47", "/"),
    ('㇓', "CJK Stroke Sp", "/"),
    ('〳', "Vertical Kana Repeat Mark Upper Half", "/"),
    ('Ⳇ', "Coptic Capital Letter Old Coptic Esh", "/"),
    ('ノ', "Katakana Letter No", "/"),
    ('丿', "CJK Unified Ideograph-4E3F", "/"),
    ('⼃', "Kangxi Radical Slash", "/"),
    ('／', "Fullwidth Solidus", "/"),

    ('＼', "Fullwidth Reverse Solidus", "\\"),
    ('﹨', "Small Reverse Solidus", "\\"),
    ('∖', "Set Minus", "\\"),
    ('⟍', "Mathematical Falling Diagonal", "\\"),
    ('⧵', "Reverse Solidus Operator", "\\"),
    ('⧹', "Big Reverse Solidus", "\\"),
    ('⧹', "Greek Vocal Notation Symbol-16", "\\"),
    ('⧹', "Greek Instrumental Symbol-48", "\\"),
    ('㇔', "CJK Stroke D", "\\"),
    ('丶', "CJK Unified Ideograph-4E36", "\\"),
    ('⼂', "Kangxi Radical Dot", "\\"),
    ('、', "Ideographic Comma", "\\"),
    ('ヽ', "Katakana Iteration Mark", "\\"),

    ('ꝸ', "Latin Small Letter Um", "&"),
    ('＆', "Fullwidth Ampersand", "&"),

    ('᛭', "Runic Cross Punctuation", "+"),
    ('➕', "Heavy Plus Sign", "+"),
    ('𐊛', "Lycian Letter H", "+"),
    ('﬩', "Hebrew Letter Alternative Plus Sign", "+"),
    ('＋', "Fullwidth Plus Sign", "+"),

    ('‹', "Single Left-Pointing Angle Quotation Mark", "<"),
    ('❮', "Heavy Left-Pointing Angle Quotation Mark Ornament", "<"),
    ('˂', "Modifier Letter Left Arrowhead", "<"),
    ('𝈶', "Greek Instrumental Symbol-40", "<"),
    ('ᐸ', "Canadian Syllabics Pa", "<"),
    ('ᚲ', "Runic Letter Kauna", "<"),
    ('❬', "Medium Left-Pointing Angle Bracket Ornament", "<"),
    ('⟨', "Mathematical Left Angle Bracket", "<"),
    ('〈', "Left-Pointing Angle Bracket", "<"),
    ('〈', "Left Angle Bracket", "<"),
    ('㇛', "CJK Stroke Pd", "<"),
    ('く', "Hiragana Letter Ku", "<"),
    ('𡿨', "CJK Unified Ideograph-21FE8", "<"),
    ('《', "Left Double Angle Bracket", "<"),
    ('＜', "Fullwidth Less-Than Sign", "<"),

    ('᐀', "Canadian Syllabics Hyphen", "="),
    ('⹀', "Double Hyphen", "="),
    ('゠', "Katakana-Hiragana Double Hyphen", "="),
    ('꓿', "Lisu Punctuation Full Stop", "="),
    ('＝', "Fullwidth Equals Sign", "="),

    ('›', "Single Right-Pointing Angle Quotation Mark", ">"),
    ('❯', "Heavy Right-Pointing Angle Quotation Mark Ornament", ">"),
    ('˃', "Modifier Letter Right Arrowhead", ">"),
    ('𝈷', "Greek Instrumental Symbol-42", ">"),
    ('ᐳ', "Canadian Syllabics Po", ">"),
    ('𖼿', "Miao Letter Archaic Zza", ">"),
    ('❭', "Medium Right-Pointing Angle Bracket Ornament", ">"),
    ('⟩', "Mathematical Right Angle Bracket", ">"),
    ('〉', "Right-Pointing Angle Bracket", ">"),
    ('〉', "Right Angle Bracket", ">"),
    ('》', "Right Double Angle Bracket", ">"),
    ('＞', "Fullwidth Greater-Than Sign", ">"),
    ('⩵', "Two Consecutive Equals Signs", "==")
];

const ASCII_ARRAY: &[(&str, &str, Option<TokenKind>)] = &[
    (" ", "Space", None),
    ("_", "Underscore", Some(TokenKind::Ident(sym::underscore))),
    ("-", "Minus/Hyphen", Some(TokenKind::BinOp(BinOpToken::Minus))),
    (",", "Comma", Some(TokenKind::Comma)),
    (";", "Semicolon", Some(TokenKind::Semi)),
    (":", "Colon", Some(TokenKind::Colon)),
    ("!", "Exclamation Mark", Some(TokenKind::Not)),
    ("?", "Question Mark", Some(TokenKind::Question)),
    (".", "Period", Some(TokenKind::Dot)),
    ("(", "Left Parenthesis", Some(TokenKind::OpenDelim(Delimiter::Parenthesis))),
    (")", "Right Parenthesis", Some(TokenKind::CloseDelim(Delimiter::Parenthesis))),
    ("[", "Left Square Bracket", Some(TokenKind::OpenDelim(Delimiter::Bracket))),
    ("]", "Right Square Bracket", Some(TokenKind::CloseDelim(Delimiter::Bracket))),
    ("{", "Left Curly Brace", Some(TokenKind::OpenDelim(Delimiter::Brace))),
    ("}", "Right Curly Brace", Some(TokenKind::CloseDelim(Delimiter::Brace))),
    ("*", "Asterisk", Some(TokenKind::BinOp(BinOpToken::Star))),
    ("/", "Slash", Some(TokenKind::BinOp(BinOpToken::Slash))),
    ("\\", "Backslash", None),
    ("&", "Ampersand", Some(TokenKind::BinOp(BinOpToken::And))),
    ("+", "Plus Sign", Some(TokenKind::BinOp(BinOpToken::Plus))),
    ("<", "Less-Than Sign", Some(TokenKind::Lt)),
    ("=", "Equals Sign", Some(TokenKind::Eq)),
    ("==", "Double Equals Sign", Some(TokenKind::EqEq)),
    (">", "Greater-Than Sign", Some(TokenKind::Gt)),
    // FIXME: Literals are already lexed by this point, so we can't recover gracefully just by
    // spitting the correct token out.
    ("\'", "Single Quote", None),
    ("\"", "Quotation Mark", None),
];

pub(super) enum TokenSubstitution {
    DirectedQuotes {
        span: Span,
        suggestion: String,
        ascii_str: &'static str,
        ascii_name: &'static str,
    },
    Other {
        span: Span,
        suggestion: String,
        ch: String,
        u_name: &'static str,
        ascii_str: &'static str,
        ascii_name: &'static str,
    },
}

pub(super) fn check_for_substitution(
    reader: &Lexer<'_, '_>,
    pos: BytePos,
    ch: char,
    count: usize,
) -> (Option<TokenKind>, Option<TokenSubstitution>) {
    let Some(&(_, u_name, ascii_str)) = UNICODE_ARRAY.iter().find(|&&(c, _, _)| c == ch) else {
        return (None, None);
    };

    let span = Span::new(pos, pos + BytePos::from_usize(ch.len_utf8() * count));

    let Some((_, ascii_name, token)) = ASCII_ARRAY.iter().find(|&&(s, _, _)| s == ascii_str) else {
        panic!("substitution character not found for {ch:?} ({ascii_str:?})");
    };

    // special help suggestion for "directed" double quotes
    let sugg = if let Some(s) = peek_delimited(&reader.src[reader.src_index(pos)..], '“', '”') {
        let c = '”'.len_utf8();
        let span = Span::new(pos, pos + BytePos::from_usize(c + s.len() + c));
        Some(TokenSubstitution::DirectedQuotes {
            span,
            suggestion: format!("\"{s}\""),
            ascii_str,
            ascii_name,
        })
    } else {
        let suggestion = ascii_str.to_string().repeat(count);
        Some(TokenSubstitution::Other {
            span,
            suggestion,
            ch: ch.to_string(),
            u_name,
            ascii_str,
            ascii_name,
        })
    };
    (token.clone(), sugg)
}

/// Extract string if found at current position with given delimiters
fn peek_delimited(text: &str, from_ch: char, to_ch: char) -> Option<&str> {
    let mut chars = text.chars();
    let first_char = chars.next()?;
    if first_char != from_ch {
        return None;
    }
    let last_char_idx = chars.as_str().find(to_ch)?;
    Some(&chars.as_str()[..last_char_idx])
}
