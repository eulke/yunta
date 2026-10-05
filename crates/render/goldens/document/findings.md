# findings of `review`: 1 blocking, 1 major

### ✗ blocking — An empty argument list panics

unwrap, at src/main.rs:2

The first argument is read with `unwrap`.

### ▲ major — A name with a newline breaks the line

newline, at src/greet.rs:4

`hello` prints the name as given, so `ana\nbob` greets on two lines.
