//! LLVM 15 through its C API: the declarations, the handles and one `Ty` for every type.

mod builder;
mod sys;
mod ty;

#[cfg(test)]
mod tests;
