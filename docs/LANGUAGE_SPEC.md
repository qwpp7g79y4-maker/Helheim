# Helheim CodeTaal: Formal Language Specification

Helheim utilizes a Native Bilingual DSL (Domain Specific Language) known as **CodeTaal**. It allows developers to seamlessly write high-performance logic in both English and Dutch. 

CodeTaal is compiled directly to an Abstract Syntax Tree (AST) and then lowered to native PTX (Nvidia GPU assembly) or executed via the fast CPU orchestrator.

## 1. Bilingual Keywords
The Helheim Parser transparently maps both languages to the same semantic tokens.

| Dutch | English | Description |
|---|---|---|
| `zet` | `let` / `set` | Variable definition |
| `als` ... `dan` | `if` ... `then` | Conditional branching |
| `anders` | `else` | Conditional fallback |
| `zolang` | `while` / `repeat` | Loop construction |
| `voor elke` | `for each` | Iteration |
| `functie` / `met` | `function` / `fn` / `with`| Function definition |
| `geef_terug` / `retourneer` | `return` | Return from function or block |
| `waar` / `onwaar` | `true` / `false` | Boolean literals |
| `roep_aan` | `call` / `invoke` | Function invocation |
| `voer uit` | `execute` | Host OS bash execution (Motor Cortex) |
| `druk_af` | `print` / `log` | Standard output |
| `lees` / `schrijf` | `read` / `write` | File I/O |
| `stuur` ... `naar` | `send` ... `to` | HSP network socket sending |
| `haal` | `fetch` | HTTP fetching |
| `probeer` / `vang` | `try` / `catch` | Error handling |
| `gedeeld` | `shared` | GPU shared memory allocation |

## 2. Types & Literals
Helheim natively supports the following types:
- **Int**: `10`, `-5`
- **Float**: `3.14`
- **String**: `"Hello World"`
- **Bool**: `waar`, `onwaar` (or `true`, `false`)
- **List**: `[1, 2, 3]` or `[waar, onwaar]` (Optimized for Spiking Neural Networks)
- **Dict**: JSON-compatible dictionaries.
- **Tensor**: Native multidimensional allocations for Matrix Math.

## 3. Operations & Expressions
Helheim utilizes a **Top-Down Operator Precedence Parser (Pratt Parser)** for evaluating mathematical and logical expressions. This ensures expressions are evaluated securely and correctly according to standard mathematical laws without explicit parentheses.

### Operator Precedence Table
Operators with higher precedence are evaluated first.

| Precedence Level | Operators | Description | Associativity |
|---|---|---|---|
| **20** | `*`, `/`, `%` | Multiplication, Division, Modulo | Left |
| **10** | `+`, `-` | Addition, Subtraction | Left |
| **7** | `<<`, `>>` | Bitwise Shift Left/Right | Left |
| **6** | `&`, `\|`, `^` | Bitwise AND, OR, XOR (SNN Spikes) | Left |
| **5** | `==`, `!=`, `<`, `>`, `<=`, `>=` | Comparison / Equality | Left |
| **3** | `&&` / `en` | Logical AND | Left |
| **2** | `\|\|` / `of` | Logical OR | Left |

*Example:* 
`zet x = 10 + 5 * 2 > 15;`
1. Evaluates `5 * 2` (10) due to Precedence 20.
2. Evaluates `10 + 10` (20) due to Precedence 10.
3. Evaluates `20 > 15` (waar) due to Precedence 5.

### Bitwise Operators (SNN Focused)
For neural network thresholding, Helheim supports bitwise operators on `Bool` lists (packed into `u32` integers natively):
- `&` (AND)
- `|` (OR)
- `^` (XOR)
- `<<`, `>>` (Bit Shifts)

### Intrinsics
- `popc(x)` / `tel_spikes(x)`: Translates directly to the `popc.b32` PTX hardware instruction to count the number of high bits (spikes) in a bit-mask.

## 4. Context Binding & Lowering
Variables defined in the host scope (e.g., `zet x = 10;`) are automatically bound to the execution context. When a block of code is selected for JIT execution on the GPU, these variables are injected into the kernel as `.param` inputs, completely bypassing Python interpreter-style symbol lookups.
