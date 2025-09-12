# Contributing to Solar

:balloon: Thanks for your help improving the project! We are so happy to have
you!

There are opportunities to contribute to Solar at any level. It doesn't matter if
you are just getting started with Rust or are the most experienced expert, we can
use your help.

**No contribution is too small and all contributions are valued.**

This guide will help you get started. **Do not let this guide intimidate you**.
It should be considered a map to help you navigate the process.

The [dev channel][dev] is available for any concerns not covered in this guide, please join us!

## Conduct

The Solar project adheres to the [Rust Code of Conduct][coc]. This describes
the _minimum_ behavior expected from all contributors. Instances of violations of the
Code of Conduct can be reported by contacting the project team at
[georgios@paradigm.xyz](mailto:georgios@paradigm.xyz).

[coc]: https://github.com/rust-lang/rust/blob/master/CODE_OF_CONDUCT.md

## Contributing in Issues

For any issue, there are fundamentally three ways an individual can contribute:

1. By opening the issue for discussion: For instance, if you believe that you
   have discovered a bug in Solar, creating a new issue in [the paradigmxyz/solar
   issue tracker][issue] is the way to report it.

2. By helping to triage the issue: This can be done by providing
   supporting details (a test case that demonstrates a bug), providing
   suggestions on how to address the issue, or ensuring that the issue is tagged
   correctly.

3. By helping to resolve the issue: Typically this is done either in the form of
   demonstrating that the issue reported is not a problem after all, or more
   often, by opening a Pull Request that changes some bit of something in
   Solar in a concrete and reviewable manner.

[issue]: https://github.com/paradigmxyz/solar/issues

**Anybody can participate in any stage of contribution**. We urge you to
participate in the discussion around bugs and participate in reviewing PRs.

### Asking for General Help

If you have reviewed existing documentation and still have questions or are
having problems, you can [open a discussion] asking for help.

In exchange for receiving help, we ask that you contribute back a documentation
PR that helps others avoid the problems that you encountered.

[open a discussion]: https://github.com/paradigmxyz/solar/discussions/new

### Submitting a Bug Report

When opening a new issue in the Solar issue tracker, you will be presented
with a basic template that should be filled in. If you believe that you have
uncovered a bug, please fill out this form, following the template to the best
of your ability. Do not worry if you cannot answer every detail, just fill in
what you can.

The two most important pieces of information we need in order to properly
evaluate the report are a description of the behavior you are seeing and a simple
test case we can use to recreate the problem on our own. If we cannot recreate
the issue, it becomes impossible for us to fix.

In order to rule out the possibility of bugs introduced by userland code, test
cases should be limited, as much as possible, to using only Solar APIs.

See [How to create a Minimal, Complete, and Verifiable example][mcve].

[mcve]: https://stackoverflow.com/help/mcve

### Triaging a Bug Report

Once an issue has been opened, it is not uncommon for there to be discussion
around it. Some contributors may have differing opinions about the issue,
including whether the behavior being seen is a bug or a feature. This discussion
is part of the process and should be kept focused, helpful, and professional.

Short, clipped responses—that provide neither additional context nor supporting
detail—are not helpful or professional. To many, such responses are simply
annoying and unfriendly.

Contributors are encouraged to help one another make forward progress as much as
possible, empowering one another to solve issues collaboratively. If you choose
to comment on an issue that you feel either is not a problem that needs to be
fixed, or if you encounter information in an issue that you feel is incorrect,
explain why you feel that way with additional supporting context, and be willing
to be convinced that you may be wrong. By doing so, we can often reach the
correct outcome much faster.

### Resolving a Bug Report

In the majority of cases, issues are resolved by opening a Pull Request. The
process for opening and reviewing a Pull Request is similar to that of opening
and triaging issues, but carries with it a necessary review and approval
workflow that ensures that the proposed changes meet the minimal quality and
functional guidelines of the Solar project.

## Pull Requests

Pull Requests are the way concrete changes are made to the code, documentation,
and dependencies in the Solar repository.

Even tiny pull requests (e.g., one character pull request fixing a typo in API
documentation) are greatly appreciated. Before making a large change, it is
usually a good idea to first open an issue describing the change to solicit
feedback and guidance. This will increase the likelihood of the PR getting
merged.

### Cargo Commands

Due to the extensive use of features in Solar, you will often need to add extra
arguments to many common cargo commands. This section lists some commonly needed
commands.

Most `cargo` subcommands can be run normally; the `--workspace` flag can be skipped
to ignore benchmarks and examples:

```
cargo check --workspace
cargo clippy --workspace
cargo fmt --all --check
cargo test --workspace
```

For running tests, we recommend using [`cargo-nextest`][cargo-nextest] to make tests run faster:

[cargo-nextest]: https://nexte.st/

```
cargo install --locked cargo-nextest
cargo nextest run --workspace
```

When building documentation, a simple `cargo doc` is not sufficient. To produce
documentation equivalent to what will be produced in docs.rs's builds of Solar's
docs, please use:

```
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --workspace --no-deps --all-features [--open]
```

This turns on indicators to display the Cargo features required for
conditionally compiled APIs in Solar.

There is a more concise way to build docs.rs-equivalent docs by using
[`cargo docs-rs`], which reads the above documentation flags out of
Solar's Cargo.toml as docs.rs itself does.

[`cargo docs-rs`]: https://github.com/dtolnay/cargo-docs-rs

```
cargo install --locked cargo-docs-rs
cargo +nightly docs-rs -p solar-compiler [--open]
```

### Spellcheck

You can perform spell-check on the codebase with the following commands:

```
cargo install --locked typos-cli
typos
```

For details of how to use `typos`, see <https://github.com/crate-ci/typos>.

If the command rejects a word, you should backtick the rejected word if it's code related.
If not, the  rejected word should be inserted into `typos.toml`. 

### Tests

If the change being proposed alters code (as opposed to only documentation for
example), it is either adding new functionality to Solar or it is fixing
existing, broken functionality. In both of these cases, the pull request should
include one or more tests to ensure that Solar does not regress in the future.
There are a few ways to write tests:
- [unit tests][unit-tests]
- [documentation tests][documentation-tests]
- [snapshot tests][snapshot-tests]
- [integration tests][integration-tests]

Unit, documentation, and snapshot tests are used to test individual library functions or modules, whereas
integration tests are used to test the compiler binary.

#### Snapshot Tests

Snapshot tests are a subset of unit tests that capture some specific output and compare it to a snapshot, usually defined inline in the test itself.

We use `snapbox` as the snapshot testing framework, which does not require any external binaries to be installed.

You can automatically create or update the snapshots by running tests normally with the `SNAPSHOTS=overwrite` environment variable,
optionally specifying the crate or test name, as you would with `cargo test` normally.
For example:
```bash
SNAPSHOTS=overwrite cargo test -p solar-ast
```

#### Integration Tests

Integration tests are located in the `tests` directory. They are run using the
[`ui_test`][ui_test] test harness, which is inspired by [`compiletest`][compiletest],
the rustc test harness.

These tests are run by default when running `cargo test` or `cargo nextest run`.

To run them specifically, you can use `cargo uitest`.

Here's a simple example to show how to write a "UI" integration test (`tests/ui` directory):

```rust
// Directives
//@compile-flags: --flag

// Annotations specify the errors that the compiler is expected to emit.
// These are `//~`, one of HELP, NOTE, WARN, ERROR, and a colon (`:`),
// followed by the expected message. The message can be a partial match.

line with error //~ ERROR: error message

// The annotation can be prefixed with any number of `^` or `v`
// to point at the N'th line above or below respectively.

//~vv ERROR: error message

line with error

// Diagnostics pointing to the same line should be grouped together using `|`.

line with multiple errors
//~^ ERROR: first error
//~| ERROR: second error
//~| ERROR: third error
```

Once you have written your test, or existing tests' output has changed, you must
run `cargo uibless` to update the expected output files.

For detailed information on how to write integration tests, see the
[`ui_test`][ui_test] and [`compiletest`][compiletest] documentation.

[unit-tests]: https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html
[documentation-tests]: https://doc.rust-lang.org/rust-by-example/testing/doc_testing.html
[integration-tests]: https://doc.rust-lang.org/rust-by-example/testing/integration_testing.html
[ui_test]: https://github.com/oli-obk/ui_test
[compiletest]: https://rustc-dev-guide.rust-lang.org/tests/compiletest.html

### Benchmarks

Check out the [`benches`](/benches) directory for information about benchmarks.

### Commits

It is a recommended best practice to keep your changes as logically grouped as
possible within individual commits. There is no limit to the number of commits
any single Pull Request may have, and many contributors find it easier to review
changes that are split across multiple commits.

That said, if you have a number of commits that are "checkpoints" and don't
represent a single logical change, please squash those together.

Note that multiple commits often get squashed when they are landed (see the
notes about [commit squashing](#commit-squashing)).

#### Commit message guidelines

Commit messages should follow the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
specification.

Sample complete commit message:

```txt
feat(module): explain the commit in one line

Body of commit message is a few lines of text, explaining things
in more detail, possibly giving some background about the issue
being fixed, etc.

The body of the commit message can be several paragraphs, and
please do proper word-wrap and keep columns shorter than about
72 characters or so. That way, `git log` will show things
nicely even when it is indented.

Fixes: #1337
Refs: #453, #154
```

### Discuss and update

You will probably get feedback or requests for changes to your Pull Request.
This is a big part of the submission process so don't be discouraged! Some
contributors may sign off on the Pull Request right away, others may have
more detailed comments or feedback. This is a necessary part of the process
in order to evaluate whether the changes are correct and necessary.

**Any community member can review a PR and you might get conflicting feedback**.
Keep an eye out for comments from code owners to provide guidance on conflicting
feedback.

**Once the PR is open, do not rebase the commits**. See [Commit Squashing](#commit-squashing) for
more details.

### Commit Squashing

In most cases, **do not squash commits that you add to your Pull Request during
the review process**. When the commits in your Pull Request land, they may be
squashed into one commit per logical change. Metadata will be added to the
commit message (including links to the Pull Request, links to relevant issues,
and the names of the reviewers). The commit history of your Pull Request,
however, will stay intact on the Pull Request page.

## Reviewing Pull Requests

**Any Solar community member is welcome to review any pull request**.

All Solar contributors who choose to review and provide feedback on Pull
Requests have a responsibility to both the project and the individual making the
contribution. Reviews and feedback must be helpful, insightful, and geared
towards improving the contribution as opposed to simply blocking it. If there
are reasons why you feel the PR should not land, explain what those are. Do not
expect to be able to block a Pull Request from advancing simply because you say
"No" without giving an explanation. Be open to having your mind changed. Be open
to working with the contributor to make the Pull Request better.

Reviews that are dismissive or disrespectful of the contributor or any other
reviewers are strictly counter to the Code of Conduct.

When reviewing a Pull Request, the primary goals are for the codebase to improve
and for the person submitting the request to succeed. **Even if a Pull Request
does not land, the submitters should come away from the experience feeling like
their effort was not wasted or unappreciated**. Every Pull Request from a new
contributor is an opportunity to grow the community.

### Review a bit at a time.

Do not overwhelm new contributors.

It is tempting to micro-optimize and make everything about relative performance,
perfect grammar, or exact style matches. Do not succumb to that temptation.

Focus first on the most significant aspects of the change:

1. Does this change make sense for Solar?
2. Does this change make Solar better, even if only incrementally?
3. Are there clear bugs or larger scale issues that need attending to?
4. Is the commit message readable and correct? If it contains a breaking change
   is it clear enough?

Note that only **incremental** improvement is needed to land a PR. This means
that the PR does not need to be perfect, only better than the status quo. Follow
up PRs may be opened to continue iterating.

When changes are necessary, *request* them, do not *demand* them, and **do not
assume that the submitter already knows how to add a test or run a benchmark**.

Specific performance optimization techniques, coding styles and conventions
change over time. The first impression you give to a new contributor never does.

Nits (requests for small changes that are not essential) are fine, but try to
avoid stalling the Pull Request. Most nits can typically be fixed by the Solar
Collaborator landing the Pull Request but they can also be an opportunity for
the contributor to learn a bit more about the project.

It is always good to clearly indicate nits when you comment: e.g.
`Nit: change foo() to bar(). But this is not blocking.`

If your comments were addressed but were not folded automatically after new
commits or if they proved to be mistaken, please, [hide them][hiding-a-comment]
with the appropriate reason to keep the conversation flow concise and relevant.

### Be aware of the person behind the code

Be aware that *how* you communicate requests and reviews in your feedback can
have a significant impact on the success of the Pull Request. Yes, we may land
a particular change that makes Solar better, but the individual might just not
want to have anything to do with Solar ever again. The goal is not just having
good code.

### Abandoned or Stalled Pull Requests

If a Pull Request appears to be abandoned or stalled, it is polite to first
check with the contributor to see if they intend to continue the work before
checking if they would mind if you took it over (especially if it just has nits
left). When doing so, it is courteous to give the original contributor credit
for the work they started (either by preserving their name and email address in
the commit log, or by using an `Author: ` meta-data tag in the commit.

_Adapted from the [Tokio contributing guide][tokio]_.

[dev]: https://t.me/paradigm_solar
[hiding-a-comment]: https://help.github.com/articles/managing-disruptive-comments/#hiding-a-comment
[tokio]: https://github.com/tokio-rs/tokio/blob/master/CONTRIBUTING.md
