# Tips and Best Practices

## Performance

### Setting `opt-level`
Both the proptest crate and the random number generator it uses can be CPU intensive. If you are
generating a lot of cases you may see a significant performance improvement by setting the `opt-level`
to `3` in your `Cargo.toml` file:

```toml
[profile.test.package.proptest]
opt-level = 3

[profile.test.package.rand_chacha]
opt-level = 3
```

### Reusing mutable resources 
Sometimes you may want to reuse mutable resources across individual cases. For example, you may want
to reuse a database connection or a file handle to avoid the overhead of opening and closing it for
each case. Because the `proptest!` macro (when used with closure-style invocation) requires a `Fn`, you need to wrap your state in a `RefCell`:

```rust
# extern crate proptest;
use std::cell::RefCell;
use proptest::proptest;

# struct ConnectionPool {};
# struct MyConnection {};
# impl ConnectionPool {
#    fn new() -> Self { Self {} }
#    fn connect(&mut self) -> MyConnection { MyConnection {} }
# }
#[test]
# fn dummy() {}; // This is here to make the doctest work
fn test_with_shared_connection() {
    let mut my_conn = RefCell::new(ConnectionPool::new().connect());
    proptest!(|(x in 0..42)| {
        let mut conn = my_conn.borrow_mut();
        // Use state
    });
}
```