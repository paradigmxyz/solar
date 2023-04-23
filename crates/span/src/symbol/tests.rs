use super::*;

#[test]
fn interner_tests() {
    let i = Interner::new();
    // first one is zero:
    assert_eq!(i.intern("dog"), Symbol::new(0));
    // re-use gets the same entry:
    assert_eq!(i.intern("dog"), Symbol::new(0));
    // different string gets a different #:
    assert_eq!(i.intern("cat"), Symbol::new(1));
    assert_eq!(i.intern("cat"), Symbol::new(1));
    // dog is still at zero
    assert_eq!(i.intern("dog"), Symbol::new(0));
}
