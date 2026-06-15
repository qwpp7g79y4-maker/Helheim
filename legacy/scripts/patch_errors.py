import re

file_path = '/home/bitboi/dev_2/Helheim/helheim-lang/src/parser.rs'
with open(file_path, 'r') as f:
    content = f.read()

method = """    pub fn format_parse_error(input: &str, token: &Token, msg: &str) -> anyhow::Error {
        let lines: Vec<&str> = input.lines().collect();
        let line_content = if token.line > 0 && token.line <= lines.len() {
            lines[token.line - 1]
        } else {
            ""
        };
        
        let mut marker = String::new();
        if token.column > 1 {
            for _ in 1..token.column {
                marker.push(' ');
            }
        }
        marker.push_str("^--- ");
        marker.push_str(msg);

        anyhow::anyhow!(
            "Syntax Fout [Regel {}, Kolom {}]: {}\\n  |\\n{} | {}\\n  | {}",
            token.line,
            token.column,
            msg,
            token.line,
            line_content,
            marker
        )
    }
"""

if "pub fn format_parse_error" not in content:
    content = content.replace('impl HelParser {', 'impl HelParser {\n' + method, 1)

content = re.sub(
    r'anyhow::anyhow!\("Fout op regel \{\}: \{\}", ([^,]+)\.line, "([^"]+)"\)',
    r'Self::format_parse_error(input, &\1, "\2")',
    content
)

content = re.sub(
    r'anyhow::anyhow!\("Fout op regel \{\}: ([^"]+)", ([^)]+)\.line\)',
    r'Self::format_parse_error(input, &\2, "\1")',
    content
)

content = content.replace(
    'anyhow::anyhow!("Fout op regel {}: Verwacht \'{{\'", start.line)',
    'Self::format_parse_error(input, &start, "Verwacht \'{\'")'
)

content = re.sub(
    r'anyhow::anyhow!\("Fout op regel \{\}: Ongeldige grootte: \{\}", ([^,]+)\.line, ([^\)]+)\)',
    r'Self::format_parse_error(input, &\1, &format!("Ongeldige grootte: {}", \2))',
    content
)

content = content.replace(
    'anyhow::anyhow!("Verwacht een bestandsnaam na \'gebruik\' of \'use\'")',
    'Self::format_parse_error(input, &token, "Verwacht een bestandsnaam na \'gebruik\' of \'use\'")'
)

content = content.replace(
    'anyhow::anyhow!("Fout: Verwacht \'[\' na \'#\'")',
    'Self::format_parse_error(input, &token, "Verwacht \'[\' na \'#\'")'
)

with open(file_path, 'w') as f:
    f.write(content)
print("Errors patched!")
