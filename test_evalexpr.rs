fn main() {
    let mut context = evalexpr::HashMapContext::new();
    use evalexpr::ContextWithMutableVariables;
    context.set_value("x".into(), evalexpr::Value::Int(10)).unwrap();
    context.set_value("y".into(), evalexpr::Value::Int(20)).unwrap();

    let res = evalexpr::eval_with_context("x == 10 && y == 20", &context);
    println!("res1 = {:?}", res);
}
