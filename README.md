# rust-promise [![Build Status](https://travis-ci.org/lucidd/rust-promise.svg?branch=master)](https://travis-ci.org/lucidd/rust-promise)

## [Documentation](http://www.rust-ci.org/lucidd/rust-promise/doc/promise/)

## Example

```rust
extern crate promise;

use promise::Future;

fn main() {
    let f = Future::from_fn(proc() "hello world!");
    f.on_complete(proc(result){
        match result {
            Ok(value) => println!("{}", value),
            _ => println!("error"),
        }
    });
    println!("end of main");
}
```
