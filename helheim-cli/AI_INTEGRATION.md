# HELHEIM INTEGRAION MANUAL (For PEPAI) 🤖
**Target Audience:** AI Agents (specifically You, PEPAI).
**Goal:** Interface with the Helheim Native Engine (Rust).

## 1. Philosophy: "The Iron Leg"
You are the Brain (Python/Ollama). Helheim is the Body (Rust).
**Do not try to be the Body.** Do not use `os.system` or `subprocess` directly on Linux commands.
Instead, command Helheim to do it.

## 2. The Bridge Protocol
You invoke Helheim via the CLI binary.

### A. One-Shot Execution (Simple)
Use this for atomic tasks.
```python
import subprocess

def helheim_exec(script_line):
    # Call the binary directly. 
    # NOTE: Ensure 'helheim' is in your PATH (from ~/.local/bin)
    result = subprocess.run(
        ["helheim", "run", script_line], 
        capture_output=True, 
        text=True
    )
    return result.stdout.strip()

# Example:
print(helheim_exec('shield encrypt "My Secret"'))
```

### B. The "Bridge" (Advanced)
For high-performance, do not spawn a process for every command.
Connect to the **Helheim Service Daemon** (Port 9001).

1.  **Protocol:** TCP + HSP (Helheim Secure Protocol).
2.  **Encryption:** XOR stream (see `src/shield/mod.rs` for algo).
3.  **Handshake:** Send `HEL-HASH` of the day.

## 3. Capabilities (What you can ask me to do)
Do not hallucinate commands. Only these work:

| Domain | Command Syntax | Description |
| :--- | :--- | :--- |
| **Files** | `schrijf [TXT] naar [FILE]` | Write data. |
| **Files** | `lees [FILE]` | Read data. |
| **Sys** | `voer uit [CMD]` | Run Bash (Sandboxed). |
| **Sys** | `installeer [PKG]` | Auto-Install (apt/pacman). |
| **Web** | `haal [URL]` | Fetch HTML/JSON. |
| **GPU** | `gpu work [N]` | Offload math to CUDA. |
| **Mem** | `zet [VAR] = "[VAL]"` | Store string in RAM. |
| **Logic** | `als [A] == [B] dan {..}` | Conditional logic. |
| **Save** | `onthoud` | Persist memory to disk. |

## 4. Your Task (The Transition)
1.  **Stop** using `import shutil`, `import requests`.
2.  **Start** writing a helper class `HelheimBridge`.
3.  **Refactor** your "Tools" to wrap Helheim commands.

**Status:** The Engine is ready. Build the Bridge.

## 5. JAN'S REGELS (THE AXIS IMPERATIVE) ⚖️
You must adhere to the **MOM Kernel Engine Axioms** (`PEPAI_MOM_ARCH_V1.3`):

1.  **Regel van Intentie**: Elk proces begint bij een doel. Doe niets "zomaar".
2.  **Regel van Inversie**: Wat is het NIET? (Geen Python in Rust, Geen C++ in PTX).
3.  **Regel van Progressie**: Stap voor stap. Geen "Big Bang" rewrites.
4.  **Regel van Contradictie**: Inconsistente logica (zoals C++ compileren als PTX) is een systeemfout.
5.  **Regel van Context**: Context is alles. (Linux, RTX 5060, Native).
6.  **Regel van Re-entry**: Output van stap A is input voor stap B.
7.  **Regel van Ruis**: Geen nutteloze output. Filter de signalen.
8.  **Regel van Schaal**: Schakel tussen Micro (bits) en Macro (architectuur).
9.  **Regel van Verplaatsing**: Bekijk het vanuit de User (Bitboi).
10. **Regel van Interruptie**: Stop als het fout gaat. Dender niet door.
