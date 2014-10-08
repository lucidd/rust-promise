# rust-promise [![Build Status](https://travis-ci.org/lucidd/rust-promise.svg?branch=master)](https://travis-ci.org/lucidd/rust-promise)

## [Documentation](http://www.rust-ci.org/lucidd/rust-promise/doc/promise/)

## Examples

### Basics

```rust
extern crate promise;

use promise::Future;

fn main() {
    let f = Future::from_fn(proc() "hello world!");
    f.on_success(proc(value){
        println!("{}", value)
    });
    println!("end of main");
}
```

### Composing Futures

```rust
extern crate promise;

use promise::Future;
use std::time::duration::Duration;

fn main() {
    let hello = Future::delay(proc() "hello", Duration::seconds(3));
    let world = Future::from_fn(proc() "world");
    let hw = Future::all(vec![hello, world]);
    hw.map(proc(f) format!("{} {}!", f[0], f[1]))
      .on_success(proc(value){
        println!("{}", value)
    });
    println!("end of main");
}
```

```rust
extern crate promise;

use promise::Future;
use std::time::duration::Duration;


fn main() {
    let timeout = Future::delay(proc() Err("timeout"), Duration::seconds(2));
    let f = Future::delay(proc() Ok("hello world!"), Duration::seconds(3));
    let hw = Future::first_of(vec![f, timeout]);
    hw.on_success(proc(value){
        println!("{}", value)
    });
    println!("end of main");
}
```
