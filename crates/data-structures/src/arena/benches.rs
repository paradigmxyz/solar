extern crate test;

use super::{tests::*, TypedArena};
use test::Bencher;

#[bench]
pub fn bench_copy(b: &mut Bencher) {
    let arena = TypedArena::default();
    b.iter(|| arena.alloc(Point { x: 1, y: 2, z: 3 }))
}

#[bench]
pub fn bench_copy_nonarena(b: &mut Bencher) {
    b.iter(|| {
        let _: Box<_> = Box::new(Point { x: 1, y: 2, z: 3 });
    })
}

#[bench]
pub fn bench_typed_arena_clear(b: &mut Bencher) {
    let mut arena = TypedArena::default();
    b.iter(|| {
        arena.alloc(Point { x: 1, y: 2, z: 3 });
        arena.clear();
    })
}

#[bench]
pub fn bench_typed_arena_clear_100(b: &mut Bencher) {
    let mut arena = TypedArena::default();
    b.iter(|| {
        for _ in 0..100 {
            arena.alloc(Point { x: 1, y: 2, z: 3 });
        }
        arena.clear();
    })
}

#[bench]
pub fn bench_noncopy(b: &mut Bencher) {
    let arena = TypedArena::default();
    b.iter(|| {
        arena.alloc(Noncopy { string: "hello world".to_string(), array: vec![1, 2, 3, 4, 5] })
    })
}

#[bench]
pub fn bench_noncopy_nonarena(b: &mut Bencher) {
    b.iter(|| {
        let _: Box<_> =
            Box::new(Noncopy { string: "hello world".to_string(), array: vec![1, 2, 3, 4, 5] });
    })
}
