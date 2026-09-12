# A Rust match made in hell

Amos Wenger

Feb 11, 2022 30 min [#rust](https://fasterthanli.me/tags/rust)

👋 This page was last updated ~5 years ago. Just so you know.

I often write pieces that showcase how well Rust can work for you, and how it can let you build powerful abstractions, and prevents you from making a bunch of mistakes.

If you read something like [Some mistakes Rust doesn’t catch](https://fasterthanli.me/articles/some-mistakes-rust-doesnt-catch) in isolation, it could seem as if I had only nice things to say about Rust, and it’s a perfect little fantasy land where nothing ever goes wrong.

Well today, let’s take a look at a footgun that cost me, infamous Rust advocate, suspected paid shill (I mean… [kinda](https://www.patreon.com/fasterthanlime)?), about a week.

[## The basics](#the-basics)

Let’s start with a little Rust syntax refresher!

Rust has `if` and `else`, like you’d expect from an imperative language:



```rust
fn is_good() -> bool {
    true
}

fn main() {
    if is_good() {
        println!("It is good");
    } else {
        println!("It isn't good, yet");
    }
}
```



```shell
$ cargo run --quiet
It is good
```

They’re not statements though, they’re expressions! Which makes up for the lack of a ternary operator (`cond ? if_true : if_false`):



```rust
fn is_good() -> bool {
    true
}

fn main() {
    let msg = if is_good() {
        "It is good"
    } else {
        "It isn't good, yet"
    };
    println!("{msg}");
}
```

And because it’s an expression, it can be used anywhere:



```rust
fn is_good() -> bool {
    true
}

fn main() {
    println!(
        "{}",
        if is_good() {
            "It is good"
        } else {
            "It isn't good, yet"
        }
    )
}
```

But Rust also has `match`, which is sort of like a more powerful `switch`:



```rust
fn is_good() -> bool {
    true
}

fn main() {
    let msg = match is_good() {
        true => "It is good",
        false => "It isn't good, yet",
    };

    println!("{msg}")
}
```

![Cool bear](https://cdn.fasterthanli.me/content/img/reimena/coolbear-neutral~7010eabf1d70d303.jxl)

It doesn’t *look* more powerful here.

Sure! But you can match the “scrutinee” (`x` in `match x { ... }`) with various patterns, in various “arms” (`true => {}`, `false => {}` in the example above).



```rust
use rand::Rng;

fn main() {
    let msg = match rand::thread_rng().gen_range(0..=10) {
        // match only 10
        10 => "Overwhelming victory",
        // match anything 5 or above
        5.. => "Victory",
        // match anything else (fallback case)
        _ => "Defeat",
    };

    println!("{msg}")
}
```



```shell
$ cargo run --quiet
Victory

$ cargo run --quiet
Overwhelming victory

$ cargo run --quiet
Defeat
```

And there’s more to `match`, as we’ll see later.

Rust also has `enum` types. They’re not just a list of integer or string values, they’re actual sum types.

Say we have a function called `process`, that can work securely or non-securely:



```rust
fn process(secure: bool) {
    if secure {
        println!("No hackers plz");
    } else {
        println!("Come on in");
    }
}

fn main() {
    process(false)
}
```

It’s kinda hard to tell from the call site (`process(false)`) what that parameter is doing. If you’re using [Rust Analyzer](https://rust-analyzer.github.io/) in an editor that supports Inlay Hints, like [VS Code](https://code.visualstudio.com/), it’s a little more obvious, because it adds the parameter name:

![](https://cdn.fasterthanli.me/content/articles/a-rust-match-made-in-hell/assets/inlay-hint~86f45f2be4d3fd9e.jxl)

But still, even with just two variants like that, I’d much rather have an enum:



```rust
pub enum Protection {
    Secure,
    Insecure,
}

fn process(prot: Protection) {
    match prot {
        Protection::Secure => {
            println!("No hackers plz");
        }
        Protection::Insecure => {
            println!("Come on in");
        }
    }
}

fn main() {
    process(Protection::Insecure)
}
```

Because that way, the call site is readable even outside of an IDE (say, while reviewing a PR on GitHub, an MR on GitLab, or a patch by e-mail 🥲)

And also, because `Protection::Secure` and `Protection::Insecure` are unique symbols (as opposed to `true` and `false`), from the comfort of my IDE, I can find places that do insecure processing by asking it to find References to `Protection::Insecure`:

![](https://cdn.fasterthanli.me/content/articles/a-rust-match-made-in-hell/assets/references~8e192a0f11394a91.jxl)

I can also, for example, mark it as deprecated, which will generate a warning wherever it’s used, without changing the type signature:



```rust
pub enum Protection {
    Secure,
    #[deprecated = "using secure mode everywhere is now strongly recommended"]
    Insecure,
}

fn process(prot: Protection) {
    match prot {
        Protection::Secure => {
            println!("No hackers plz");
        }
        // We still need to handle this case
        #[allow(deprecated)]
        Protection::Insecure => {
            println!("Come on in");
        }
    }
}

fn main() {
    process(Protection::Insecure)
}
```



```shell
$ cargo check
    Checking lox v0.1.0 (/home/amos/bearcove/lox)
warning: use of deprecated unit variant `Protection::Insecure`: using secure mode everywhere is now strongly recommended
  --> src/main.rs:21:25
   |
21 |     process(Protection::Insecure)
   |                         ^^^^^^^^
   |
   = note: `#[warn(deprecated)]` on by default

warning: `lox` (bin "lox") generated 1 warning
    Finished dev [unoptimized + debuginfo] target(s) in 0.28s
```

And because I have the [Error Lens](https://marketplace.visualstudio.com/items?itemName=usernamehw.errorlens) extension, it even shows up inline:

![](https://cdn.fasterthanli.me/content/articles/a-rust-match-made-in-hell/assets/error-lens~48a96b0d178a7cfc.jxl)

And if you’re wondering why I’m spending time showing off tooling, it’s because it is an integral part of “the Rust experience”: great diagnostics? that’s a feature. Being able to see all references to a symbol, or to rename a symbol easily? That’s a feature. That Java has had forever (via Eclipse/NetBeans etc.), but is extremely hard to achieve in languages like Python or C++.

![Cool bear](https://cdn.fasterthanli.me/content/img/reimena/coolbear-neutral~7010eabf1d70d303.jxl)

Isn’t C++ IDE support pretty okay by now?

![Amos](https://cdn.fasterthanli.me/content/img/reimena/amos-neutral~55a3477398fe0cb1.jxl)

It’s not non-existent, but several language features makes it really really hard to make it better.

![Cool bear](https://cdn.fasterthanli.me/content/img/reimena/coolbear-neutral~7010eabf1d70d303.jxl)

Doesn’t Rust have the same issue with macros?

![Amos](https://cdn.fasterthanli.me/content/img/reimena/amos-neutral~55a3477398fe0cb1.jxl)

In a way! Although [rust-analyzer](https://rust-analyzer.github.io/) is getting pretty good at it, *even proc macros*. There’s still the odd “whoops no autocomplete” or “whoops no suggested imports” occurence, but the situation has improved dramatically over the past few months.

Enum variants can have associated data:



```rust
pub enum Protection {
    Secure { version: u64 },
    Insecure,
}

fn process(prot: Protection) {
    match prot {
        Protection::Secure { version } => {
            println!("Hacker-safe thanks to protocol v{version}");
        }
        Protection::Insecure => {
            println!("Come on in");
        }
    }
}

fn main() {
    process(Protection::Secure { version: 2 })
}
```



```shell
$ cargo run --quiet                       
Hacker-safe thanks to protocol v2
```

You can see that in the first match “arm”, we *destructure* the variant, to extract “version” out of it, which ends up being a local `u64` binding. Here too, inlay hints are useful:

![](https://cdn.fasterthanli.me/content/articles/a-rust-match-made-in-hell/assets/type-hint-u64~099cddc1d53599c8.jxl)

It’s unlikely we have [18 quintillion](https://www.wolframalpha.com/input/?i=2**64) versions of our security protocol, though - although we might want to use a fixed-size integer in our actual protocol, when we deal with it in code, we might want to list them out explicitly:



```rust
pub enum Protection {
    Secure(SecureVersion),
    Insecure,
}

#[derive(Debug)]
pub enum SecureVersion {
    V1,
    V2,
    V2_1,
}

fn process(prot: Protection) {
    match prot {
        Protection::Secure(version) => {
            println!("Hacker-safe thanks to protocol {version:?}");
        }
        Protection::Insecure => {
            println!("Come on in");
        }
    }
}

fn main() {
    process(Protection::Secure(SecureVersion::V2_1))
}
```



```shell
$ cargo run --quiet
Hacker-safe thanks to protocol V2_1
```

[## Clone and Copy](#clone-and-copy)

Our `Protection` type, so far, is neither `Clone` nor `Copy`. Which means, when we pass it to `process`, it’s *moved* into it. And once a value is moved into something, we don’t have ownership of it anymore. So this doesn’t work, for example:



```rust
// omitted: enum definitions

fn main() {
    let prot = Protection::Secure(SecureVersion::V2_1);
    process(prot);
    process(prot);
}
```



```shell
$ cargo run --quiet
error[E0382]: use of moved value: `prot`
  --> src/main.rs:27:13
   |
25 |     let prot = Protection::Secure(SecureVersion::V2_1);
   |         ---- move occurs because `prot` has type `Protection`, which does not implement the `Copy` trait
26 |     process(prot);
   |             ---- value moved here
27 |     process(prot);
   |             ^^^^ value used here after move

For more information about this error, try `rustc --explain E0382`.
error: could not compile `lox` due to previous error
```

Here, `Protection` is “plain old data”, so there’s no harm in doing a bit-level copy of it somewhere else: it’ll behave the same.

So here, we can derive `Clone` and `Copy`, and instead of moving into `process`, we’ll be handing it copies:



```rust
//        👇    👇
#[derive(Clone, Copy)]
pub enum Protection {
    Secure(SecureVersion),
    Insecure,
}
```



```shell
$ cargo run --quiet
error[E0204]: the trait `Copy` may not be implemented for this type
 --> src/main.rs:1:17
  |
1 | #[derive(Clone, Copy)]
  |                 ^^^^
2 | pub enum Protection {
3 |     Secure(SecureVersion),
  |            ------------- this field does not implement `Copy`
  |
  = note: this error originates in the derive macro `Copy` (in Nightly builds, run with -Z macro-backtrace for more info)

For more information about this error, try `rustc --explain E0204`.
error: could not compile `lox` due to previous error
```

Mhhh oh right, we also need `SecureVersion` to implement `Copy`, let’s do that now:



```rust
#[derive(Clone, Copy)]
pub enum Protection {
    Secure(SecureVersion),
    Insecure,
}

//        👇    👇
#[derive(Clone, Copy, Debug)]
pub enum SecureVersion {
    V1,
    V2,
    V2_1,
}
```



```shell
$ cargo run --quiet
Hacker-safe thanks to protocol V2_1
Hacker-safe thanks to protocol V2_1
```

Deriving `Clone` and `Copy` actually makes perfect sense for these types, but just for learning’s sake, let’s not. Let’s go back to this:



```rust
pub enum Protection {
    Secure(SecureVersion),
    Insecure,
}

#[derive(Debug)]
pub enum SecureVersion {
    V1,
    V2,
    V2_1,
}

fn process(prot: Protection) {
    match prot {
        Protection::Secure(version) => {
            println!("Hacker-safe thanks to protocol {version:?}");
        }
        Protection::Insecure => {
            println!("Come on in");
        }
    }
}

fn main() {
    let prot = Protection::Secure(SecureVersion::V2_1);
    process(prot);
    process(prot);
}
```

And this compile error:



```shell
$ cargo run --quiet
error[E0382]: use of moved value: `prot`
  --> src/main.rs:27:13
   |
25 |     let prot = Protection::Secure(SecureVersion::V2_1);
   |         ---- move occurs because `prot` has type `Protection`, which does not implement the `Copy` trait
26 |     process(prot);
   |             ---- value moved here
27 |     process(prot);
   |             ^^^^ value used here after move

For more information about this error, try `rustc --explain E0382`.
error: could not compile `lox` due to previous error
```

How can we make it so we can pass the same `Protection` to `process` several times?

Well, instead of sending it “by value” (either moving or copying it), we can pass it “by reference”:



```rust
//              👇
fn process(prot: &Protection) {
    match prot {
        Protection::Secure(version) => {
            println!("Hacker-safe thanks to protocol {version:?}");
        }
        Protection::Insecure => {
            println!("Come on in");
        }
    }
}

fn main() {
    let prot = Protection::Secure(SecureVersion::V2_1);
    //     👇
    process(&prot);
    //     👇
    process(&prot);
}
```



```shell
$ cargo run --quiet
Hacker-safe thanks to protocol V2_1
Hacker-safe thanks to protocol V2_1
```

That worked nicely!

We had to change the function signature of `process`, but we didn’t have to change the `match` whatsoever: it’s still just this:



```rust
    match prot {
        Protection::Secure(version) => {
            println!("Hacker-safe thanks to protocol {version:?}");
        }
        Protection::Insecure => {
            println!("Come on in");
        }
    }
```

Which means this works just as well, whether `prot` is a `Protection`, or whether it’s a `&Protection`. That’s… interesting. Very interesting.

Here’s a question for you: what’s the type of `version` in the first match arm?

I don’t have to wonder, because I’m looking at this inlay hint:

![](https://cdn.fasterthanli.me/content/articles/a-rust-match-made-in-hell/assets/match-ref~3a5b284a46dd0082.jxl)

Which is… interesting. We’re able to match on `Protection` as well as `&Protection`. In the “value” case, we’ll get a `SecureVersion` when we destructure, and in the “reference” case we’ll get a `&SecureVersion`.

Which seems like a good thing! Because it’s exactly what I wanted to do.

And now, for something completely different.

[## Locks!](#locks)

Rust doesn’t let you create more than one “mutable reference” to something at a time. That’s one way in which it enforces memory safety.

For example, this doesn’t build:



```rust
fn main() {
    let mut counter = 0_u64;

    crossbeam::scope(|s| {
        for _ in 0..3 {
            s.spawn(|_| {
                counter += 1;
            });
        }
    })
    .unwrap();
}
```

Because we can’t have three threads mutate the same value willy-nilly.



```shell
$ cargo run --quiet
error[E0499]: cannot borrow `counter` as mutable more than once at a time
 --> src/main.rs:6:21
  |
4 |       crossbeam::scope(|s| {
  |                         - has type `&Scope<'1>`
5 |           for _ in 0..3 {
6 |               s.spawn(|_| {
  |               -       ^^^ `counter` was mutably borrowed here in the previous iteration of the loop
  |  _____________|
  | |
7 | |                 counter += 1;
  | |                 ------- borrows occur due to use of `counter` in closure
8 | |             });
  | |______________- argument requires that `counter` is borrowed for `'1`

For more information about this error, try `rustc --explain E0499`.
error: could not compile `lox` due to previous error
```

One way to solve is with a `Mutex`. And in Rust, mutexes typically own the “protected” data, so the type we want here is `Mutex<u64>`:



```rust
use parking_lot::Mutex;

fn main() {
    let counter = Mutex::new(0_u64);

    crossbeam::scope(|s| {
        for _ in 0..3 {
            s.spawn(|_| {
                // let's increment it a "couple" times
                for _ in 0..100_000 {
                    *counter.lock() += 1;
                }
            });
        }
    })
    .unwrap();

    let counter = counter.into_inner();
    println!("final count: {counter}");
}
```



```shell
$ cargo run --quiet                                              
final count: 300000
```

The type system, and the design of the `Mutex` API, enforce that we can ONLY read or write the value *if* we hold a lock.

The signature for `parking_lot::Mutex::lock` is as follows:

`pub fn lock(&self) -> MutexGuard<'_, R, T> {`

Which means it:

- takes an immutable reference to the `Mutex`
- returns a `MutexGuard` that lives for no longer than the immutable reference to the `Mutex`
- …parameterized by the lock type `R` (here a `RawMutex`)
- …and by the type protected by the lock (here an `u64`)

And `MutexGuard`, in turn, implements the [Deref](https://doc.rust-lang.org/stable/std/ops/trait.Deref.html) and [DerefMut](https://doc.rust-lang.org/stable/std/ops/trait.DerefMut.html) traits, which means it acts as a smart pointer to `T`.

In other words, with tedious type annotations, it lets us do this:



```rust
use parking_lot::Mutex;

fn main() {
    let counter = Mutex::new(0_u64);

    let mut guard = counter.lock();

    // Using `DerefMut`
    let mutable_ref: &mut u64 = &mut guard;
    *mutable_ref = 42;

    // Using `Deref`
    let immutable_ref: &u64 = &guard;
    dbg!(immutable_ref);
}
```



```shell
$ cargo run --quiet
[src/main.rs:13] immutable_ref = 42
```

Another thing Rust protects us against is dangling references! Here, the lifetime of the guard is bound to the lifetime of the Mutex itself, so, for example, that code doesn’t compile:



```rust
use parking_lot::Mutex;

fn main() {
    let mut guard = {
        let counter = Mutex::new(0_u64);
        let guard = counter.lock();

        guard
    };

    *guard = 42;
    dbg!(*guard);
}
```



```shell
$ cargo run --quiet
error[E0597]: `counter` does not live long enough
 --> src/main.rs:6:21
  |
4 |     let mut guard = {
  |         --------- borrow later stored here
5 |         let counter = Mutex::new(0_u64);
6 |         let guard = counter.lock();
  |                     ^^^^^^^^^^^^^^ borrowed value does not live long enough
...
9 |     };
  |     - `counter` dropped here while still borrowed

For more information about this error, try `rustc --explain E0597`.
error: could not compile `lox` due to previous error
```

As you may have noticed, the Rust compiler says “to borrow” instead of “to create a reference to”.

Which brings us to our next question: sure, the mutex is locked when `Mutex::lock` is called. But when is it unlocked? Well, when the `MutexGuard` drops!



```rust
fn main() {
    let nd = NoisyDrop;
    println!("before drop...");
    drop(nd);
    println!("after drop!");
}

struct NoisyDrop;

impl Drop for NoisyDrop {
    fn drop(&mut self) {
        println!("dropping!");
    }
}
```



```shell
$ cargo run --quiet
before drop...
dropping!
after drop!
```

`drop` is the simplest function: it takes ownership of a value and throws it away:

`fn drop<T>(t: T) {}`

Values also drop at the end of a scope: we can achieve the same output with this program:



```rust
fn main() {
    {
        let nd = NoisyDrop;
        println!("before drop...");
        // `nd` goes out of scope and is dropped.
    }
    println!("after drop!");
}
```

And now it’s time for… a quiz!

Does this work?



```rust
fn main() {
    let x = 42;

    let a = &x;
    println!("a = {a}");

    let b = &x;
    println!("b = {b}");
}
```

Yes it does!

Does this?



```rust
fn main() {
    let mut x = 42;

    let a = &mut x;
    println!("a = {a}");

    let b = &mut x;
    println!("b = {b}");
}
```

Also yes!

Does this?



```rust
fn main() {
    let mut x = 42;

    let a = &mut x;
    println!("a = {a}");

    let b = &mut x;
    println!("b = {b}");

    *a = 707;
}
```

No!



```shell
$ cargo run --quiet
error[E0499]: cannot borrow `x` as mutable more than once at a time
  --> src/main.rs:7:13
   |
4  |     let a = &mut x;
   |             ------ first mutable borrow occurs here
...
7  |     let b = &mut x;
   |             ^^^^^^ second mutable borrow occurs here
...
10 |     *a = 707;
   |     -------- first borrow later used here

For more information about this error, try `rustc --explain E0499`.
error: could not compile `lox` due to previous error
```

How about this?



```rust
#[derive(Debug)]
struct Fahrenheit<'a, T>(&'a mut T);

fn main() {
    let mut x = 451;

    let a = Fahrenheit(&mut x);
    println!("a = {a:?}");

    let b = Fahrenheit(&mut x);
    println!("b = {b:?}");
}
```



```shell
$ cargo run --quiet
a = Fahrenheit(451)
b = Fahrenheit(451)
```

What if our type has a `Drop` impl?



```rust
#[derive(Debug)]
struct Chrome<'a, T>(&'a mut T, &'static str);

impl<'a, T> Drop for Chrome<'a, T> {
    fn drop(&mut self) {
        let name = self.1;
        println!("dropping {name}");
    }
}

fn main() {
    let mut x = 98;

    let a = Chrome(&mut x, "a");
    println!("a = {a:?}");

    let b = Chrome(&mut x, "b");
    println!("b = {b:?}");
}
```

Then it doesn’t work anymore!



```shell
$ cargo run --quiet
error[E0499]: cannot borrow `x` as mutable more than once at a time
  --> src/main.rs:17:20
   |
14 |     let a = Chrome(&mut x, "a");
   |                    ------ first mutable borrow occurs here
...
17 |     let b = Chrome(&mut x, "b");
   |                    ^^^^^^ second mutable borrow occurs here
18 |     println!("b = {b:?}");
19 | }
   | - first borrow might be used here, when `a` is dropped and runs the `Drop` code for type `Chrome`

For more information about this error, try `rustc --explain E0499`.
error: could not compile `lox` due to previous error
```

The diagnostic explains exactly why, so I don’t have to do it.

In a previous version of the article, there was code like this, which worked, and seemed a little confusing:



```rust
fn main() {
    let mut x = 98;

    let a = Chrome(&mut x, "a");
    dbg!(a);

    let b = Chrome(&mut x, "b");
    dbg!(b);
}
```

The short explanation is `dbg!(a)` drops `a`. The slightly longer explanation is it it prints the expression and *then* evaluates to that expression, so that it can be used in places where values are being moved, like this:



```rust
// this is a String 👇, an owned type
do_thing(dbg!(s.to_string()));
// both `s.to_string()` and `dbg!(s.to_string())` evaluate to the same value.
```

In other words, the code above is more or less equivalent to this:



```rust
fn main() {
    let mut x = 98;

    let a = Chrome(&mut x, "a");
    do_some_debug_printing(&a);
    a;

    let b = Chrome(&mut x, "b");
    do_some_debug_printing(&b);
    b;
}
```

Where `a;` and `b;` are the statements that *actually* drop `a` and `b`.

clippy actually warns about naked “path statements” like these:



```shell
$ cargo clippy
warning: path statement drops value
  --> src/main.rs:16:5
   |
16 |     a;
   |     ^^ help: use `drop` to clarify the intent: `drop(a);`
   |
   = note: `#[warn(path_statements)]` on by default

warning: path statement drops value
  --> src/main.rs:20:5
   |
20 |     b;
   |     ^^ help: use `drop` to clarify the intent: `drop(b);`
```

Now let’s talk about methods!

You can define methods on types you own in `impl` blocks.



```rust
#[derive(Debug)]
struct Foobar(i64);

impl Foobar {
    fn negated(self) -> i64 {
        -self.0
    }
}

fn main() {
    let f = Foobar(72);
    println!("{f:?}");

    let a = f.negated();
    println!("{a}");
}
```



```shell
$ cargo run --quiet
Foobar(72)
-72
```

That method we just defined has `self` as a receiver: it takes ownership of `Foobar`.

What we wrote here:



```rust
impl Foobar {
    fn negated(self) -> i64 {
        -self.0
    }
}
```

Is the short version of this:



```rust
impl Foobar {
    fn negated(self: Self) -> i64 {
        -self.0
    }
}
```

Which is itself the short version of this:



```rust
impl Foobar {
    fn negated(self: Foobar) -> i64 {
        -self.0
    }
}
```

And `Foobar` is not `Copy`, which means calling `f.negated()` *moves `f`* elsewhere, and so, we can’t call it twice.



```rust
#[derive(Debug)]
struct Foobar(i64);

impl Foobar {
    fn negated(self) -> i64 {
        -self.0
    }
}

fn main() {
    let f = Foobar(72);
    println!("{f:?}");

    let a = f.negated();
    println!("{a}");

    let b = f.negated();
    println!("{b}");
}
```



```shell
$ cargo run --quiet
error[E0382]: use of moved value: `f`
  --> src/main.rs:17:13
   |
11 |     let f = Foobar(72);
   |         - move occurs because `f` has type `Foobar`, which does not implement the `Copy` trait
...
14 |     let a = f.negated();
   |               --------- `f` moved due to this method call
...
17 |     let b = f.negated();
   |             ^ value used here after move
   |
note: this function takes ownership of the receiver `self`, which moves `f`
  --> src/main.rs:5:16
   |
5  |     fn negated(self) -> i64 {
   |                ^^^^

For more information about this error, try `rustc --explain E0382`.
error: could not compile `lox` due to previous error
```

Instead, we can take `&self`, the short version of `self: &Self`:



```rust
impl Foobar {
    fn negated(&self) -> i64 {
        -self.0
    }
}
```

And now, we can call it twice:



```shell
$ cargo run --quiet
Foobar(72)
-72
-72
```

But here’s something interesting we can do:



```rust
#[derive(Debug)]
struct Foobar(i64);

impl Foobar {
    fn get(&self) -> &i64 {
        &self.0
    }
}

fn main() {
    let f = Foobar(134);

    let a = f.get();
    println!("a = {a}");
}
```



```shell
$ cargo run --quiet
a = 134
```

And that’s very, *very* interesting.

Because one might think that this ought not to work:



```rust
    fn get(&self) -> &i64 {
        &self.0
    }
```

One might argue that `&i64` can only live for as long as `&self`, and so, once we return, `&self` stops living, and so we cannot possibly return a reference to some subset of `&self`.

Which is true… except the trick is simply to extend the lifetime of `&self` to however long the return value is used.

See, we can’t drop `f` before we’re done with `a`: because it’s still borrowed.



```rust
#[derive(Debug)]
struct Foobar(i64);

impl Foobar {
    fn get(&self) -> &i64 {
        &self.0
    }
}

fn main() {
    let f = Foobar(134);
    let a = f.get();
    drop(f);
    println!("a = {a}");
}
```



```shell
$ cargo run --quiet
error[E0505]: cannot move out of `f` because it is borrowed
  --> src/main.rs:13:10
   |
12 |     let a = f.get();
   |             ------- borrow of `f` occurs here
13 |     drop(f);
   |          ^ move out of `f` occurs here
14 |     println!("a = {a}");
   |                   --- borrow later used here

For more information about this error, try `rustc --explain E0505`.
error: could not compile `lox` due to previous error
```

And this is called a “borrow-through”.

It’s even more visible with mutable borrows:



```rust
#[derive(Debug)]
struct Foobar(i64);

impl Foobar {
    fn get_mut(&mut self) -> &mut i64 {
        &mut self.0
    }
}

fn main() {
    let mut f = Foobar(134);

    let a = f.get_mut();
    f.0 = 278;
    println!("a = {a}");
}
```



```shell
$ cargo run --quiet
error[E0506]: cannot assign to `f.0` because it is borrowed
  --> src/main.rs:14:5
   |
13 |     let a = f.get_mut();
   |             ----------- borrow of `f.0` occurs here
14 |     f.0 = 278;
   |     ^^^^^^^^^ assignment to borrowed `f.0` occurs here
15 |     println!("a = {a}");
   |                   --- borrow later used here

For more information about this error, try `rustc --explain E0506`.
error: could not compile `lox` due to previous error
```

[## A couple deadlocks](#a-couple-deadlocks)

Let’s get back to mutexes.

If we try to acquire locks on a mutex twice in a row, our program gets stuck:



```rust
use parking_lot::Mutex;

fn main() {
    let l = Mutex::new(0);

    println!("acquiring a...");
    let _a = l.lock();
    println!("acquiring a... done!");

    println!("acquiring b...");
    let _b = l.lock();
    println!("acquiring b... done!");
}
```



```shell
$ cargo run --quiet
acquiring a...
acquiring a... done!
acquiring b...
^C
```

The second lock is waiting for the first lock to release, which will never happen since we’re holding onto it.

That deadlock was pretty easy to spot. Sometimes it’s not as easy, like in this case:



```rust
use parking_lot::Mutex;

#[derive(Default)]
struct State {
    value: Mutex<u64>,
}

impl State {
    fn foo(&self) {
        println!("entering foo");
        let mut guard = self.value.lock();
        *guard += 1;
        self.bar();
        println!("exiting foo");
    }

    fn bar(&self) {
        println!("entering bar");
        let mut guard = self.value.lock();
        if *guard > 10 {
            *guard = 0
        }
        println!("exiting bar");
    }
}

fn main() {
    let s: State = Default::default();
    s.foo();
}
```



```shell
$ cargo run --quiet
entering foo
entering bar
^C
```

That one’s a little more subtle. Especially if you imagine that the real `foo` and `bar` are much larger.

Still, it’s not that bad, looking at a stack trace in GDB:



```shell
$ rust-gdb --quiet --args ./target/debug/lox
Reading symbols from ./target/debug/lox...
(gdb) r
Starting program: /home/amos/bearcove/lox/target/debug/lox 
Downloading -0.00 MB separate debug info for /home/amos/.cache/debuginfod_client/88564abce789aa42536da1247a57ff6062d61dcb/debuginfo
[Thread debugging using libthread_db enabled]                                                                                                                                       
Using host libthread_db library "/lib64/libthread_db.so.1".
entering foo
entering bar
^C
Program received signal SIGINT, Interrupt.
syscall () at ../sysdeps/unix/sysv/linux/x86_64/syscall.S:38
38              cmpq $-4095, %rax       /* Check %rax for error.  */
(gdb) bt
#0  syscall () at ../sysdeps/unix/sysv/linux/x86_64/syscall.S:38
#1  0x0000555555560ff1 in parking_lot_core::thread_parker::imp::ThreadParker::futex_wait (self=0x7ffff7d86740, ts=...)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot_core-0.9.1/src/thread_parker/linux.rs:112
#2  0x0000555555560dfc in parking_lot_core::thread_parker::imp::{impl#0}::park (self=0x7ffff7d86740)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot_core-0.9.1/src/thread_parker/linux.rs:66
#3  0x0000555555560684 in parking_lot_core::parking_lot::park::{closure#0}<parking_lot::raw_mutex::{impl#3}::lock_slow::{closure#0}, parking_lot::raw_mutex::{impl#3}::lock_slow::{closure#1}, parking_lot::raw_mutex::{impl#3}::lock_slow::{closure#2}> (thread_data=0x7ffff7d86720)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot_core-0.9.1/src/parking_lot.rs:635
#4  0x0000555555560325 in parking_lot_core::parking_lot::with_thread_data<parking_lot_core::parking_lot::ParkResult, parking_lot_core::parking_lot::park::{closure#0}> (f=...)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot_core-0.9.1/src/parking_lot.rs:207
#5  parking_lot_core::parking_lot::park<parking_lot::raw_mutex::{impl#3}::lock_slow::{closure#0}, parking_lot::raw_mutex::{impl#3}::lock_slow::{closure#1}, parking_lot::raw_mutex::{impl#3}::lock_slow::{closure#2}> (key=140737488345304, validate=..., before_sleep=..., timed_out=..., park_token=..., timeout=...)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot_core-0.9.1/src/parking_lot.rs:600
#6  0x000055555555f32f in parking_lot::raw_mutex::RawMutex::lock_slow (self=0x7fffffffd8d8, timeout=...)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot-0.12.0/src/raw_mutex.rs:262
#7  0x000055555555dc55 in parking_lot::raw_mutex::{impl#0}::lock (self=0x7fffffffd8d8)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/parking_lot-0.12.0/src/raw_mutex.rs:72
#8  0x000055555555de13 in lock_api::mutex::Mutex<parking_lot::raw_mutex::RawMutex, u64>::lock<parking_lot::raw_mutex::RawMutex, u64> (self=0x7fffffffd8d8)
    at /home/amos/.cargo/registry/src/github.com-1ecc6299db9ec823/lock_api-0.4.6/src/mutex.rs:214
#9  0x000055555555da2b in lox::State::bar (self=0x7fffffffd8d8) at src/main.rs:19
#10 0x000055555555d965 in lox::State::foo (self=0x7fffffffd8d8) at src/main.rs:13
#11 0x000055555555db21 in lox::main () at src/main.rs:29
```

…we can see that `bar` is the function that’s hanging, and we can see it’s been called from `foo` - it’s not that bad!

It’s even better with colors!

![](https://cdn.fasterthanli.me/content/articles/a-rust-match-made-in-hell/assets/gdb-colors~960dc2e1d5817889.jxl)

A good way to get rid of that particular deadlock is to push the mutex outside of our type, so that methods are defined on the already-locked version:



```rust
use parking_lot::Mutex;

#[derive(Default)]
struct State {
    // no longer in a `Mutex`
    value: u64,
}

impl State {
    // now taking `&mut`
    fn foo(&mut self) {
        self.value += 4;
        self.bar();
        println!("exiting foo, value = {}", self.value);
    }

    fn bar(&mut self) {
        if self.value > 10 {
            self.value = 0
        }
    }
}

fn main() {
    // the `Mutex` lives here now
    let s: Mutex<State> = Default::default();
    for _ in 0..5 {
        // and we must lock it to call `foo`
        s.lock().foo();
    }
}
```



```shell
$ cargo run --quiet
exiting foo, value = 4
exiting foo, value = 8
exiting foo, value = 0
exiting foo, value = 4
exiting foo, value = 8
```

With the added bonus that now, if `State` is not shared across multiple threads, we can use it without any locking whatsoever.

[## All together now](#all-together-now)

Let’s put everything we’ve learned together and look at some more code!

Does this code run?



```rust
use parking_lot::Mutex;

#[derive(Default, Debug)]
struct State {
    value: u64,
}

impl State {
    fn is_even(&self) -> bool {
        self.value % 2 == 0
    }

    fn increment(&mut self) {
        self.value += 1
    }
}

fn main() {
    let s: Mutex<State> = Default::default();
    if s.lock().is_even() {
        s.lock().increment();
    }
    dbg!(&s.lock());
}
```

Yeah! Of course it does!



```shell
$ cargo run --quiet
[src/main.rs:23] &s.lock() = State {
    value: 1,
}
```

Because by the time we call `increment()`, we’ve already released the lock we had when we called `is_even()`.

What about this code? Does this run?



```rust
fn main() {
    let s: Mutex<State> = Default::default();

    let is_even = s.lock().is_even();
    if is_even {
        s.lock().increment();
    }
    dbg!(&s.lock());
}
```

Yeah! Same deal!



```shell
$ cargo run --quiet
[src/main.rs:25] &s.lock() = State {
    value: 1,
}
```

What about this?



```rust
fn main() {
    let s: Mutex<State> = Default::default();

    let is_even = s.lock().is_even();
    match is_even {
        true => {
            s.lock().increment();
        }
        false => {
            println!("wasn't even");
        }
    }
    dbg!(&s.lock());
}
```

Of course it does! Why wouldn’t it?



```shell
$ cargo run --quiet
[src/main.rs:30] &s.lock() = State {
    value: 1,
}
```

And what about that one?



```rust
fn main() {
    let s: Mutex<State> = Default::default();

    match s.lock().is_even() {
        true => {
            s.lock().increment();
        }
        false => {
            println!("wasn't even");
        }
    }
    dbg!(&s.lock());
}
```



```shell
$ cargo run --quiet
^C
```

Huh. HUH!

![Cool bear](https://cdn.fasterthanli.me/content/img/reimena/coolbear-neutral~7010eabf1d70d303.jxl)

Huh.

That one… does not spark joy.

[## What the fuck is happening?](#what-the-fuck-is-happening)

…is a fair question, considering.

See, the thing with `match` is, it lets you do pattern matching. As opposed to `if`, which just does equality comparison.

That means with `match`, we can do something like this:



```rust
enum Node<T> {
    Left(T),
    Right(T),
}

impl<T> Node<T> {
    fn get(&self) -> &T {
        match self {
            Self::Left(x) => x,
            Self::Right(x) => x,
        }
    }
}

fn main() {
    let a = Node::Left(23);
    let b = Node::Right(47);

    fn inspect(n: &Node<u64>) {
        let msg = match n.get() {
            0 => "zero",
            _ => "non-zero",
        };
        println!("{msg}");
    }
    inspect(&a);
    inspect(&b);
}
```



```shell
$ cargo run --quiet
non-zero
non-zero
```

That’s fine - just a little “borrow-through”, we’ve seen that before. `Node::get` returns a value that borrows from `&self`.

But we can also do *this*:



```rust
fn main() {
    let a = Mutex::new(Node::Left(23));
    let b = Mutex::new(Node::Right(47));

    fn inspect(n: &Mutex<Node<u64>>) {
        let msg = match n.lock().get() {
            0 => "zero",
            _ => "non-zero",
        };
        println!("{msg}");
    }
    inspect(&a);
    inspect(&b);
}
```

And that gets… even more interesting.

Because, for example, we cannot do this:



```rust
fn main() {
    let a = Mutex::new(Node::Left(23));
    let b = Mutex::new(Node::Right(47));

    fn inspect(n: &Mutex<Node<u64>>) {
        let res = n.lock().get();
        let msg = match res {
            0 => "zero",
            _ => "non-zero",
        };
        println!("{msg}");
    }
    inspect(&a);
    inspect(&b);
}
```



```shell
$ cargo run --quiet
error[E0716]: temporary value dropped while borrowed
  --> src/main.rs:22:19
   |
22 |         let res = n.lock().get();
   |                   ^^^^^^^^      - temporary value is freed at the end of this statement
   |                   |
   |                   creates a temporary which is freed while still in use
23 |         let msg = match res {
   |                         --- borrow later used here
   |
   = note: consider using a `let` binding to create a longer lived value

For more information about this error, try `rustc --explain E0716`.
error: could not compile `lox` due to previous error
```

And yet, when the `match` expression, also called a “scrutinee”, as I’ve recently learned, is the full `n.lock().get()`, it works.

So here’s the very good reason: because `match` allows borrow-throughs, and matching against references, it extends the lifetime of temporaries to the whole match.

In other words, it does this transformation:



```rust
fn main() {
    let a = Mutex::new(Node::Left(23));
    let b = Mutex::new(Node::Right(47));

    fn inspect(n: &Mutex<Node<u64>>) {
        let msg = {
            let tmp1 = n.lock();
            let tmp2 = tmp1.get();
            match tmp2 {
                0 => "zero",
                _ => "non-zero",
            }
        };
        println!("{msg}");
    }
    inspect(&a);
    inspect(&b);
}
```

[## And now, async](#and-now-async)

So, `match` in combination with mutex guards has pretty surprising behavior!

A pretty bad surprise! I was *pretty miffed* when I found that *this* was why my code had been deadlocking all this time.

But in my case, things were even worse!

Let’s talk about async Rust. It’s really neat! It lets you do multiple things concurrently, even on a single thread:



```rust
use std::{
    io::{stdout, Write},
    time::Duration,
};
use tokio::{spawn, time::sleep};

//             👇
#[tokio::main(flavor = "current_thread")]
async fn main() {
    for name in "abc".chars() {
        spawn(async move {
            for _ in 0..5 {
                sleep(Duration::from_millis(10)).await;
                print!("{name}");
                stdout().flush().unwrap();
            }
        });
    }

    sleep(Duration::from_millis(100)).await;
    println!();
}
```



```shell
$ cargo run --quiet
abcabcabcabcabc
```

I pinky-promise there’s just one thread here! We can ask our buddy `strace` to confirm:



```shell
$ cargo build --quiet && strace -ff -e write ./target/debug/lox 2>&1 | grep -v '\0'
write(1, "a", 1a)                        = 1
write(1, "b", 1b)                        = 1
write(1, "c", 1c)                        = 1
write(1, "a", 1a)                        = 1
write(1, "b", 1b)                        = 1
write(1, "c", 1c)                        = 1
write(1, "a", 1a)                        = 1
write(1, "b", 1b)                        = 1
write(1, "c", 1c)                        = 1
write(1, "a", 1a)                        = 1
write(1, "b", 1b)                        = 1
write(1, "c", 1c)                        = 1
write(1, "a", 1a)                        = 1
write(1, "b", 1b)                        = 1
write(1, "c", 1c)                        = 1
write(1, "\n", 1
)                       = 1
```

Here’s how it would look if there were multiple threads:



```rust
//             👇
#[tokio::main(worker_threads = 2)]
async fn main() {
    // (cut)
}
```



```shell
$ cargo build --quiet && strace -ff -e write ./target/debug/lox 2>&1 | grep -v '\0'
strace: Process 2656548 attached
strace: Process 2656549 attached
[pid 2656549] write(1, "a", 1a)          = 1
[pid 2656548] write(1, "c", 1c)          = 1
[pid 2656549] write(1, "b", 1b)          = 1
[pid 2656549] write(1, "a", 1a)          = 1
[pid 2656548] write(1, "b", 1b)          = 1
[pid 2656549] write(1, "c", 1 <unfinished ...>
[pid 2656548] <... write resumed>)      = 8
c[pid 2656549] <... write resumed>)      = 1
[pid 2656549] write(1, "a", 1a)          = 1
[pid 2656548] write(1, "c", 1c)          = 1
[pid 2656549] write(1, "b", 1b)          = 1
[pid 2656549] write(1, "a", 1a)          = 1
[pid 2656548] write(1, "b", 1b)          = 1
[pid 2656549] write(1, "c", 1c)          = 1
[pid 2656548] write(1, "c", 1c)          = 1
[pid 2656548] write(1, "ab", 2ab)         = 2
[pid 2656547] write(1, "\n", 1
)         = 1
```

We can see our main thread, `47`, and our two worker threads: `48` and `49`.

But let’s go back to our single-threaded example, and actually wait until all our futures are done, instead of sleeping some arbitrary amount of time:



```rust
use futures::future::join_all;
use std::{
    io::{stdout, Write},
    time::Duration,
};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    join_all("abc".chars().map(|name| async move {
        for _ in 0..5 {
            sleep(Duration::from_millis(10)).await;
            print!("{name}");
            stdout().flush().unwrap();
        }
    }))
    .await;
}
```

Still works fine:



```shell
$ cargo run --quiet
abcabcabcabcabc
```

Now let’s add a lock! So we can collect the output in a `String` instead:



```rust
use futures::future::join_all;
use parking_lot::Mutex;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let res: Mutex<String> = Default::default();
    join_all("abc".chars().map(|name| async move {
        for _ in 0..5 {
            sleep(Duration::from_millis(10)).await;
            res.lock().push(name);
        }
    }))
    .await;
}
```



```shell
$ cargo run --quiet
error[E0507]: cannot move out of `res`, a captured variable in an `FnMut` closure
  --> src/main.rs:9:50
   |
8  |        let res: Mutex<String> = Default::default();
   |            --- captured outer variable
9  |        join_all("abc".chars().map(|name| async move {
   |  _________________________________-_________________^
   | | ________________________________|
   | ||
10 | ||         for _ in 0..5 {
11 | ||             sleep(Duration::from_millis(10)).await;
12 | ||             res.lock().push(name);
   | ||             ---
   | ||             |
   | ||             move occurs because `res` has type `parking_lot::lock_api::Mutex<parking_lot::RawMutex, String>`, which does not implement the `Copy` trait
   | ||             move occurs due to use in generator
13 | ||         }
14 | ||     }))
   | ||     ^
   | ||_____|
   | |______captured by this `FnMut` closure
   |        move out of `res` occurs here

For more information about this error, try `rustc --explain E0507`.
error: could not compile `lox` due to previous error
```

Woops, that didn’t work!

Well, that’s because our async block is `async move`, and it’s that way because we need `name` to move into the block, so that we take ownership of it. But we don’t want to take ownership of `res`! We just want to borrow it for a little while!

We can’t really have a middleground between `async {}` and `async move {}`, but what we can do… is move a *reference* to `res` inside the block instead, with this neat little trick:



```rust
use futures::future::join_all;
use parking_lot::Mutex;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let res: Mutex<String> = Default::default();
    join_all("abc".chars().map(|name| {
        let res = &res;
        async move {
            for _ in 0..5 {
                sleep(Duration::from_millis(10)).await;
                res.lock().push(name);
            }
        }
    }))
    .await;
    println!("res = {}", res.into_inner());
}
```

And boom, it works:



```shell
$ cargo run --quiet
res = abcabcabcabcabc
```

And now for something completely innocent: let’s move some things around!



```rust
use futures::future::join_all;
use parking_lot::Mutex;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let res: Mutex<String> = Default::default();
    join_all("abc".chars().map(|name| {
        let res = &res;
        async move {
            for _ in 0..5 {
                let mut guard = res.lock();
                sleep(Duration::from_millis(10)).await;
                guard.push(name);
            }
        }
    }))
    .await;
    println!("res = {}", res.into_inner());
}
```



```shell
$ cargo run --quiet
^C
```

This deadlocks. Why? Well, because here’s a REALLY high-level view of how async works. When we `.await` something, either it’s immediately ready, and we can continue, or it’s not, and we 1) subscribe to be woken up later, 2) yield.

For us to be able to be woken up later, we have to save our current state: in this case, it involves storing the `guard` somewhere.

So, here’s what happens:

- future 1 gets polled, it acquires the lock
- future 1 sleeps for 10ms: or rather, it gets a `Sleep` future that will be ready in 10 milliseconds. It’s not ready now, so it gets a `Poll::Pending` and it yields to the executor, after saving its whole state (including the mutex guard)
- future 2 gets polled, it tries to acquire the lock

At this point, we’re deadlocked. future 2 will never be able to acquire the lock, and it will never yield, so future 1 will never be polled again (which is the only way for it to release the lock).

Of course, it doesn’t have to be this way! Instead of having locks that *block*, we could have locks that yield, and “wake” the future (so it gets polled again) when they’re ready to be acquired.

That’s exactly what `tokio::sync::Mutex` does:



```rust
use futures::future::join_all;
use std::time::Duration;
//     now async!  👇
use tokio::{sync::Mutex, time::sleep};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let res: Mutex<String> = Default::default();
    join_all("abc".chars().map(|name| {
        let res = &res;
        async move {
            for _ in 0..5 {
                // now we need to await it! 👇
                let mut guard = res.lock().await;
                sleep(Duration::from_millis(10)).await;
                guard.push(name);
            }
        }
    }))
    .await;
    println!("res = {}", res.into_inner());
}
```

And that version of the code works again:



```shell
$ cargo run --quiet
res = abcabcabcabcabc
```

Here’s the more complete lesson: never hold a Mutex guard across await points, *unless* it’s for an async lock (like `tokio::sync::Mutex`).

[## The need for more, better lints](#the-need-for-more-better-lints)

Since “holding a sync mutex guard across an await point” is a correctness issue, there is actually a lint for it: [await\_holding\_lock](https://rust-lang.github.io/rust-clippy/master/#await_holding_lock).

Unfortunately, there’s several issues with it.

First off, you currently have to opt into it:



```rust
// 👇 here's the opt-in
#![warn(clippy::await_holding_lock)]

use std::{sync::Mutex, time::Duration};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let m: Mutex<u64> = Default::default();
    let mut lock = m.lock().unwrap();
    sleep(Duration::from_millis(10)).await;
    *lock += 1;
}
```



```shell
$ cargo clippy
warning: this MutexGuard is held across an 'await' point. Consider using an async-aware Mutex type or ensuring the MutexGuard is dropped before calling await
  --> src/main.rs:9:9
   |
9  |     let mut lock = m.lock().unwrap();
   |         ^^^^^^^^
   |
note: the lint level is defined here
  --> src/main.rs:1:9
   |
1  | #![warn(clippy::await_holding_lock)]
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^
note: these are all the await points this lock is held through
  --> src/main.rs:9:5
   |
9  | /     let mut lock = m.lock().unwrap();
10 | |     sleep(Duration::from_millis(10)).await;
11 | |     *lock += 1;
12 | | }
   | |_^
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#await_holding_lock

warning: `lox` (bin "lox") generated 1 warning
    Finished dev [unoptimized + debuginfo] target(s) in 0.13s
```

It wasn’t always that way: that lint used to be in the [`correctness` category](https://github.com/rust-lang/rust-clippy/#clippy), which means it was “deny” by default (it generated an error).

But because it can generate false positives, it’s now in the `pedantic` category. Here’s one example of false positive:



```rust
#![warn(clippy::await_holding_lock)]

use std::{sync::Mutex, time::Duration};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let m: Mutex<u64> = Default::default();
    let mut lock = m.lock().unwrap();
    *lock += 1;
    drop(lock);
    sleep(Duration::from_millis(10)).await;
}
```



```shell
$ cargo clippy
warning: this MutexGuard is held across an 'await' point. Consider using an async-aware Mutex type or ensuring the MutexGuard is dropped before calling await
  --> src/main.rs:9:9
   |
9  |     let mut lock = m.lock().unwrap();
   |         ^^^^^^^^
   |
note: the lint level is defined here
  --> src/main.rs:1:9
   |
1  | #![warn(clippy::await_holding_lock)]
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^
note: these are all the await points this lock is held through
  --> src/main.rs:9:5
   |
9  | /     let mut lock = m.lock().unwrap();
10 | |     *lock += 1;
11 | |     drop(lock);
12 | |     sleep(Duration::from_millis(10)).await;
13 | | }
   | |_^
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#await_holding_lock

warning: `lox` (bin "lox") generated 1 warning
    Finished dev [unoptimized + debuginfo] target(s) in 0.01s
```

I actually remember hitting this. The workaround isn’t that bad, you can just scope your lock instead of dropping it explicitly. This doesn’t work everywhere, but it’s often easy enough:



```rust
#![warn(clippy::await_holding_lock)]

use std::{sync::Mutex, time::Duration};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let m: Mutex<u64> = Default::default();
    {
        let mut lock = m.lock().unwrap();
        *lock += 1;
    }
    sleep(Duration::from_millis(10)).await;
}
```

The second unfortunate thing with this lint is that… it currently doesn’t work with `parking_lot::Mutex`:



```rust
#![warn(clippy::await_holding_lock)]

//     👇
use parking_lot::Mutex;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let m: Mutex<u64> = Default::default();
    let mut lock = m.lock();
    sleep(Duration::from_millis(10)).await;
    *lock += 1;
}
```



```shell
$ cargo clippy
    Finished dev [unoptimized + debuginfo] target(s) in 0.01s
```

Which is confusing as heck, because the [original PR](https://github.com/rust-lang/rust-clippy/pull/5439/files#diff-9bf10c866fff21dd6600e5f3f5c7148c22885a027c8dffce244be3593a6a6c45R75-R77) explicitly mentions `parking_lot`.

And thirdly, although it does catch a case like this:



```rust
#![deny(clippy::await_holding_lock)]

use futures::future::join_all;
use std::{sync::Mutex, time::Duration};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let res: Mutex<String> = Default::default();
    join_all("abc".chars().map(|name| {
        let res = &res;
        async move {
            for _ in 0..5 {
                do_stuff(name, res).await;
            }
        }
    }))
    .await;
    println!("res = {}", res.into_inner().unwrap());
}

async fn do_stuff(name: char, res: &Mutex<String>) {
    let mut guard = res.lock().unwrap();
    sleep(Duration::from_millis(10)).await;
    guard.push(name);
}
```



```shell
$ cargo clippy
    Checking lox v0.1.0 (/home/amos/bearcove/lox)
error: this MutexGuard is held across an 'await' point. Consider using an async-aware Mutex type or ensuring the MutexGuard is dropped before calling await
  --> src/main.rs:23:9
   |
23 |     let mut guard = res.lock().unwrap();
   |         ^^^^^^^^^
   |
note: the lint level is defined here
  --> src/main.rs:1:9
   |
1  | #![deny(clippy::await_holding_lock)]
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^
note: these are all the await points this lock is held through
  --> src/main.rs:23:5
   |
23 | /     let mut guard = res.lock().unwrap();
24 | |     sleep(Duration::from_millis(10)).await;
25 | |     guard.push(name);
26 | | }
   | |_^
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#await_holding_lock

error: could not compile `lox` due to previous error
```

It cannot catch a case like that:



```rust
#![deny(clippy::await_holding_lock)]

use futures::future::join_all;
use std::{sync::Mutex, time::Duration};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let res: Mutex<String> = Default::default();
    join_all("abc".chars().map(|name| {
        let res = &res;
        async move {
            for _ in 0..5 {
                let mut guard = res.lock().unwrap();
                sleep(Duration::from_millis(10)).await;
                guard.push(name);
            }
        }
    }))
    .await;
    println!("res = {}", res.into_inner().unwrap());
}
```



```shell
$ cargo clippy
    Finished dev [unoptimized + debuginfo] target(s) in 0.01s
```

And that’s *fairly* disappointing.

[## My actual bug](#my-actual-bug)

The bug I actually lost a week and a half to (well, I did refactor a bunch of things, which ended up making the codebase a lot nicer, so, no regrets) was a perfect storm of *all of the above*, plus `RwLock` instead of `Mutex`, making the deadlock even more subtle.

My code looked like this:



```rust
use std::{sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::time::sleep;

#[derive(Default)]
struct State {
    value: u64,
}

impl State {
    fn foo(&self) -> bool {
        self.value > 0
    }

    fn bar(&self) -> u64 {
        self.value
    }

    fn update(&mut self) {
        self.value += 1;
    }
}

#[tokio::main(worker_threads = 2)]
async fn main() {
    let state: Arc<RwLock<State>> = Default::default();

    tokio::spawn({
        let state = state.clone();
        async move {
            loop {
                println!("updating...");
                state.write().update();
                sleep(Duration::from_millis(1)).await;
            }
        }
    });

    for _ in 0..10 {
        match state.read().foo() {
            true => {
                println!("it's true!");
                sleep(Duration::from_millis(1)).await;
                println!("bar = {}", state.read().bar());
            }
            false => {
                println!("it's false!");
            }
        }
    }
    println!("okay done");
}
```

It doesn’t always deadlock! Sometimes it works fine:



```shell
$ cargo run
    Finished dev [unoptimized + debuginfo] target(s) in 0.02s
     Running `target/debug/lox`
updating...
it's false!
it's true!
updating...
bar = 1
it's true!
updating...
bar = 2
it's true!
updating...
bar = 3
it's true!
updating...
bar = 4
it's true!
updating...
bar = 5
it's true!
updating...
bar = 6
it's true!
updating...
bar = 7
it's true!
updating...
bar = 8
it's true!
updating...
bar = 9
okay done
```

Here’s a case where it deadlocks:



```shell
$ cargo run
    Finished dev [unoptimized + debuginfo] target(s) in 0.01s
     Running `target/debug/lox`
it's false!
updating...
it's false!
it's true!
updating...
bar = 1
it's true!
updating...
bar = 2
it's true!
updating...
```

And here’s a heavily shortened version of the traces GDB shows:



```shell
$ rust-gdb --quiet -p $(pidof lox)
Attaching to process 2907949
[Thread debugging using libthread_db enabled]
Using host libthread_db library "/lib64/libthread_db.so.1".
syscall () at ../sysdeps/unix/sysv/linux/x86_64/syscall.S:38
38              cmpq $-4095, %rax       /* Check %rax for error.  */
(gdb) set pagination off
(gdb) thread apply all backtrace

Thread 3 (Thread 0x7f5d4a244640 (LWP 2907952) "tokio-runtime-w"):
...
#6  0x0000558227c236c8 in parking_lot::raw_rwlock::RawRwLock::wait_for_readers
...
#9  0x0000558227b327c3 in lock_api::rwlock::RwLock<parking_lot::raw_rwlock::RawRwLock, lox::State>::write<parking_lot::raw_rwlock::RawRwLock, lox::State>
#10 0x0000558227b3bc1e in lox::main::{generator#0}::{generator#0} () at src/main.rs:34
...

Thread 2 (Thread 0x7f5d4a445640 (LWP 2907951) "tokio-runtime-w"):
(that thread is parked)

Thread 1 (Thread 0x7f5d4a446d00 (LWP 2907949) "lox"):
...
#9  0x0000558227b32793 in lock_api::rwlock::RwLock<parking_lot::raw_rwlock::RawRwLock, lox::State>::read<parking_lot::raw_rwlock::RawRwLock, lox::State>
#10 0x0000558227b3c32f in lox::main::{generator#0} () at src/main.rs:45
...
```

Which points to these two places:



```rust
async fn main() {
    let state: Arc<RwLock<State>> = Default::default();

    tokio::spawn({
        let state = state.clone();
        async move {
            loop {
                println!("updating...");
                //      👇
                state.write().update();
                sleep(Duration::from_millis(1)).await;
            }
        }
    });

    for _ in 0..10 {
        match state.read().foo() {
            true => {
                println!("it's true!");
                sleep(Duration::from_millis(1)).await;
                //                          👇
                println!("bar = {}", state.read().bar());
            }
            false => {
                println!("it's false!");
            }
        }
    }
    println!("okay done");
}
```

What’s happening here is that there’s an “RWR” interleaving: we’re holding a read lock, a writer is trying to acquire, but we won’t release the first read lock until we are able to acquire a *second* read lock.

And… that wasn’t obvious to me at all.

First, I didn’t understand that calling `state.read()` in a `match` scrutinee would hold the guard *through the entire match*.

Then, I knew not to hold a sync mutex guard across await points, but I was reliant on the clippy lint denying such code, because it *used* to be part of the “correctness” category, so I expected an error to pop up if I had made that mistake.

Turning on that lint would not have helped, because it doesn’t currently work with `parking_lot`.

Finally, I found out about `parking_lot`’s “deadlock detector” feature, but couldn’t use it! Because it’s incompatible with the `send_guard` feature of that same crate.

I don’t even know *which* crate in my dependency graph uses the `send_guard` feature! There’s no mention of it in the `Cargo.lock`, and I couldn’t find a `cargo-tree` invocation that answered that question.

Cool Bear from the future here: you can get `cargo-tree` to show features by adding `-e features`.

You can even combine it with `-i`, so `cargo tree -i parking_lot -e features` answered that question after the article was published.

Had I been able to use that feature, I would’ve done this:



```toml
[dependencies]
parking_lot = { version = "0.12.0", features = ["deadlock_detection"] }
```



```rust
use std::{sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::time::sleep;

#[derive(Default)]
struct State {
    value: u64,
}

impl State {
    fn foo(&self) -> bool {
        self.value > 0
    }

    fn bar(&self) -> u64 {
        self.value
    }

    fn update(&mut self) {
        self.value += 1;
    }
}

#[tokio::main(worker_threads = 2)]
async fn main() {
    #[cfg(feature = "deadlock_detection")]
    {
        // only for #[cfg]
        use parking_lot::deadlock;
        use std::thread;
        use std::time::Duration;

        // Create a background thread which checks for deadlocks every 10s
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(10));
            let deadlocks = deadlock::check_deadlock();
            if deadlocks.is_empty() {
                continue;
            }

            println!("{} deadlocks detected", deadlocks.len());
            for (i, threads) in deadlocks.iter().enumerate() {
                println!("Deadlock #{}", i);
                for t in threads {
                    println!("Thread Id {:#?}", t.thread_id());
                    println!("{:#?}", t.backtrace());
                }
            }
        });
    } // only for #[cfg]

    let state: Arc<RwLock<State>> = Default::default();

    tokio::spawn({
        let state = state.clone();
        async move {
            loop {
                println!("updating...");
                state.write().update();
                sleep(Duration::from_millis(1)).await;
            }
        }
    });

    for _ in 0..10 {
        match state.read().foo() {
            true => {
                println!("it's true!");
                sleep(Duration::from_millis(1)).await;
                println!("bar = {}", state.read().bar());
            }
            false => {
                println!("it's false!");
            }
        }
    }
    println!("okay done");
}
```

And… that wouldn’t have helped either:



```shell
$ cargo run
   Compiling lox v0.1.0 (/home/amos/bearcove/lox)
    Finished dev [unoptimized + debuginfo] target(s) in 0.96s
     Running `target/debug/lox`
updating...
it's false!
it's true!
updating...
```

I waited for a whole minute, and no prints. Maybe I’m holding it wrong, maybe it only works for `Mutex` and not `RwLock`, maybe there’s a good reason it’s marked “experimental” - I’m not sure.

In the end, I fixed it the good old-fashioned way: by reading and reading and reading the code again, and refactoring it until the most suspicious part was the `match`, and I came up with a small repro that clarified my understanding of the semantics.

Then I fixed it, like this:



```rust
    for _ in 0..10 {
        //  👇
        let res = state.read().foo();
        //    👇
        match res {
            true => {
                println!("it's true!");
                sleep(Duration::from_millis(1)).await;
                println!("bar = {}", state.read().bar());
            }
            false => {
                println!("it's false!");
            }
        }
    }
```

And that was it. No more deadlocks.

[## What happens now?](#what-happens-now)

As soon as I [brought this up on Twitter](https://twitter.com/fasterthanlime/status/1491803585385877519), a bunch of things happened.

Mostly, everyone agreed that the fact that this code deadlocks:



```rust
match mutex.lock().foo() {
    true => {
        mutex.lock().bar();
    }
    false => {}
}
```

is surprising in the worst way.

And so [an issue was opened](https://github.com/rust-lang/rust/issues/93883), to add a lint for “unexpectedly late drop for temporaries in match scrutinee expressions”. Someone offered to mentor whoever wants to tackle that issue, and mentioned discussions [from 2016](https://github.com/rust-lang/rust/issues/37612).

Someone else linked to [a clippy issue from 2017](https://github.com/rust-lang/rust-clippy/issues/1904) about temporaries being used for values for which dropping has side effects.

As of 2022-02-12, a PR to make the lint work with `parking_lot` again [has been opened](https://github.com/rust-lang/rust-clippy/pull/8419).

I feel good about this.

It feels like the conversation has been re-opened. When I brought it up, nobody got defensive. Nobody felt like I was trash-talking their favorite sports team.

Instead, I was met with empathy, and immediately a half-dozen folks started suggesting solutions, offering to mentor, etc. Now it’s just a matter of someone stepping up and actually implementing those.

Because this is not a fatal flaw of the language, some corner we’ve painted ourselves into. This is a usability issue we can remedy without breaking the whole language, just like the countless diagnostics people like [Esteban](https://twitter.com/ekuber) has added or improved *almost every time I publish an article*.

We could change the semantics in the Rust 2024 edition! Is it worth it? I don’t know! In the meantime, we could add a lint for it. Would that be enough? It would be a *big* improvement!

For me, even “just” having the clippy “await\_while\_holding\_lock” lint fixed (to work with `parking_lot` again, and not have false positives when guards are dropped explicitly), and have it promoted to the “correctness” category again, would be huge.

Or maybe this shouldn’t be a clippy lint at all? Since it’s correctness-related. There’s work around a [`#[must_not_suspend]`](https://github.com/rust-lang/rust/issues/83310) attribute that would bring that check directly into rustc. (It’s not stabilized yet at the time of this writing)>

With it, this is a compile error:



```rust
#![feature(must_not_suspend)]

use std::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let m: Mutex<u64> = Default::default();
    do_stuff(&m).await;
}

async fn do_stuff(m: &Mutex<u64>) {
    let mut guard = m.lock();
    sleep(Duration::from_millis(10)).await;
    *guard += 1;
}
```

The tooling around rustc and clippy looks *awesome*. Having seasoned Rust developers offer mentorship is a rare learning opportunity. Why don’t you give it a shot?

If you don’t, well, heck, I just might.

(JavaScript is required to see this. Or maybe my stuff broke)
