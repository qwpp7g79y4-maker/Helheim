pub mod ast;
pub mod semantic;
pub mod synthesis;
pub mod parser;
pub mod resolver;
pub mod memory;
pub mod persistence;
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
