use helheim_lang::parser::HelParser;

fn main() {
    let content = std::fs::read_to_string("stdlib/pure/http.hel").unwrap();
    let ast = HelParser::parse(&content).unwrap();
    println!("{:#?}", ast);
}
