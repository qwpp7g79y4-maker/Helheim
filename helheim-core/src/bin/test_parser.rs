fn main() {
    let script = r#"
        perform Actor.spawn("...");
        roep_aan wacht 1;
    "#;
    let ast = helheim_lang::parser::HelParser::parse(script).unwrap();
    println!("{:#?}", ast);
}
