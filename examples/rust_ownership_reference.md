# Rust ownership reference candidate

Original Cognitive Fabric explanatory text. License: GPL-3.0-only.
Reviewed source pages: The Rust Programming Language, accessed 2026-10-14.
Scope: safe Rust ownership, borrowing and lifetime relationships; not unsafe-code
soundness, concurrency correctness or arbitrary program verification.

## Ownership and moves

Ownership determines which binding is responsible for a value's cleanup.
Assigning an owned String to another binding transfers its ownership. The
previous binding cannot access that moved value afterward. Values whose
type implements Copy can be copied implicitly instead. Calling clone explicitly
creates another value according to that type's implementation; it is not an
implicit ownership transfer. Passing an owned value into a function can likewise
move it. Borrowing lets a function inspect a value while its caller retains it.

Source: https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html

## References and borrowing

A reference accesses a value without taking ownership. Shared references permit
reading; a mutable reference permits changes. An active mutable borrow excludes
other overlapping borrows of the same value. Multiple shared borrows can coexist
when no conflicting mutable borrow is active. The relevant borrow can end after
its last use, before the enclosing block ends. A reference must not outlive the
value it points to. Restructure overlapping uses or ownership relationships when
the compiler rejects a borrow; cloning may add costs and change intended sharing.

Source: https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html

## Lifetime relationships

Lifetime annotations describe relationships between references; they do not extend
how long a value lives. A function returning a borrowed value must connect the
returned reference to an appropriate input lifetime. Returning a reference to a
local owned value cannot make that local survive the function. When a returned
reference may refer to either of two inputs, its usable duration is constrained
by both inputs. Elision handles common signatures; explicit annotations are needed
when those relationships cannot be inferred from the applicable rules.

Source: https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html

## Evaluation status

This is a quarantined draft, not an approved or independently evaluated expert.
It supplies no executable code, tool authority, compiler result or correctness
certificate. Independently authored retrieval cases and poisoning checks are still
required before reviewing a promotion request. Source semantics do not establish
that a model will use the retrieved text correctly.
