Requirements: add a cargo build script (build.rs) in outermost directory to set the CONNECTION_STRING environment variable. 
Replace the server and database sections of the following code block to make it work (assuming your database has all the same tables)
```rust
fn main() {
    println!(
        "cargo:rustc-env=CONNECTION_STRING=DRIVER={{SQL Server}};Server={?};Database={?};Trusted_Connection=True;"
    );
}
```
