import re

with open("helheim-core/src/orchestra/executor.rs", "r") as f:
    content = f.read()

# We want to replace the `for i in 0..ast.len()` loop with `TrampolineStack`.

start_idx = content.find("fn execute_ast_internal(")
if start_idx == -1:
    print("Could not find execute_ast_internal")
    exit(1)

body_start = content.find("Box::pin(async move {", start_idx)
if body_start == -1:
    print("Could not find Box::pin")
    exit(1)

for_loop_start = content.find("for i in 0..ast.len() {", body_start)

# Let's extract the inside of the for loop.
