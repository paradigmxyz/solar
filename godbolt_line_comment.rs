#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawTokenKind {
    LineComment { is_doc: bool },
}

struct Cursor<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    fn prev(&self) -> u8 {
        if self.pos == 0 { 0 } else { self.input[self.pos - 1] }
    }

    fn first(&self) -> u8 {
        self.input.get(self.pos).copied().unwrap_or(0)
    }

    fn second(&self) -> u8 {
        self.input.get(self.pos + 1).copied().unwrap_or(0)
    }

    fn bump(&mut self) {
        if self.pos < self.input.len() {
            self.pos += 1;
        }
    }

    fn eat_until_either(&mut self, a: u8, b: u8) {
        while self.pos < self.input.len() {
            let ch = self.input[self.pos];
            if ch == a || ch == b {
                break;
            }
            self.pos += 1;
        }
    }

    #[inline(never)]
    fn line_comment(&mut self) -> RawTokenKind {
        debug_assert!(self.prev() == b'/' && self.first() == b'/');
        self.bump();

        // `////` (more than 3 slashes) is not considered a doc comment.
        let is_doc = matches!(self.first(), b'/' if self.second() != b'/');

        // Take into account Windows line ending (CRLF)
        self.eat_until_either(b'\n', b'\r');
        RawTokenKind::LineComment { is_doc }
    }
}

fn main() {
    let input = b"// regular comment\n/// doc comment\n//// not doc\n";
    let mut cursor = Cursor::new(input);

    // Skip to first comment
    cursor.pos = 1;
    let token1 = cursor.line_comment();
    println!("{:?}", token1);
}