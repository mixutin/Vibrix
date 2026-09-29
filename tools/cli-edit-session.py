"""One-session source edits; removed before the feature PR is finalized."""
from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    target = Path(path)
    source = target.read_text()
    if source.count(old) != count:
        raise RuntimeError(f"source changed: {path}: expected {count} exact matches")
    target.write_text(source.replace(old, new))


runtime = "userspace/shell/src/runtime.rs"
replace(runtime, "io.read_dir(path.bytes(), 0, &mut abi::DirEntry::EMPTY)", "directory(io, path.bytes())", 3)
replace(runtime, "fn require_empty(args:", "fn directory(io: &mut dyn System, path: &[u8]) -> vibrix_syscall::Result<bool> {\n    let mut entry = abi::DirEntry::EMPTY;\n    io.read_dir(path, 0, &mut entry)\n}\n\nfn require_empty(args:")
replace(runtime, "b\"copy requires a regular file\"", "b\"operation requires a regular file\"")
replace(runtime,
        "let input = input_path.as_ref().map(|path| io.open(path.bytes(), abi::OPEN_READ)).transpose()?;",
        "let input = input_path.as_ref().map(|path| { require_regular(io, path.bytes())?; io.open(path.bytes(), abi::OPEN_READ).map_err(Error::from) }).transpose()?;")
replace(runtime,
        "let path = Path::resolve(&self.cwd, arg)?;\n                        let fd = io.open(path.bytes(), abi::OPEN_READ)?;",
        "let path = Path::resolve(&self.cwd, arg)?;\n                        require_regular(io, path.bytes())?;\n                        let fd = io.open(path.bytes(), abi::OPEN_READ)?;")
replace(runtime,
        "let path = Path::resolve(&self.cwd, arg)?;\n                    io.open(path.bytes(), abi::OPEN_READ).map_err(Error::from)",
        "let path = Path::resolve(&self.cwd, arg)?;\n                    require_regular(io, path.bytes())?;\n                    io.open(path.bytes(), abi::OPEN_READ).map_err(Error::from)")
replace(runtime,
        'for page in MANUALS {\n            write_all(io, fd, page.name)?;\n            write_all(io, fd, b" - ")?;\n            write_all(io, fd, page.summary)?;\n            write_all(io, fd, b"\\n")?;\n        }',
        'for (index, page) in MANUALS.iter().enumerate() {\n            write_all(io, fd, page.name)?;\n            if index % 5 == 4 || index + 1 == MANUALS.len() {\n                write_all(io, fd, b"\\n")?;\n            } else {\n                for _ in page.name.len()..12 { write_all(io, fd, b" ")?; }\n            }\n        }')
replace(runtime,
        'for part in [page.name, b"(1) - Vibrix userspace\\n\\nNAME\\n  ", page.name, b" - ", page.summary, b"\\n\\nSYNOPSIS\\n  ", page.usage, b"\\n\\nDESCRIPTION\\n  ", page.description, b"\\n\\nEXAMPLES\\n  ", page.example, b"\\n\\nEXIT STATUS\\n  0 success; 1 failure/no match; 2 usage; see description.\\n\\nLIMITS\\n  Built-in, no external binary. RAM files are not persistent.\\n  Text filters: 1024 bytes; sort/uniq: 128 lines. See man shell.\\n"] {\n        write_all(io, fd, part)?;\n    }',
        'for part in [page.name, b"(1) - Vibrix userspace\\n\\nNAME\\n  ", page.name, b" - ", page.summary, b"\\n\\nSYNOPSIS\\n  ", page.usage, b"\\n\\nDESCRIPTION\\n  "] { write_all(io, fd, part)?; }\n    let mut column = 2;\n    for word in page.description.split(|byte| byte.is_ascii_whitespace()).filter(|word| !word.is_empty()) {\n        if column > 2 {\n            if column + 1 + word.len() > 74 { write_all(io, fd, b"\\n  ")?; column = 2; }\n            else { write_all(io, fd, b" ")?; column += 1; }\n        }\n        write_all(io, fd, word)?;\n        column += word.len();\n    }\n    for part in [&b"\\n\\nEXAMPLES\\n  "[..], page.example, b"\\n\\nEXIT STATUS\\n  0 success; 1 failure/no match; 2 usage; see description.\\n\\nLIMITS\\n  Built-in, no external binary. RAM files are not persistent.\\n  Text filters: 1024 bytes; sort/uniq: 128 lines. See man shell.\\n"] { write_all(io, fd, part)?; }')
replace("kernel/src/arch/x86_64/ps2.rs", "    fn_control_u_and_pause", "    fn control_u_and_pause")
replace("kernel/src/memory/address_space.rs",
        "let stack_layout = GuardedLayout::new(\n        Page::new_user(STACK_GUARD).map_err(|e| ElfProbeError::AddressSpace(e.into()))?,\n        1,\n    )",
        "// The interactive shell owns bounded history, parser and text buffers.\n    // Keep both guard pages and user RW/NX permissions; other ELF probes\n    // retain their original one-page stack and the frame pool is unchanged.\n    let stack_pages = if cfg!(feature = \"rust-shell-probe\") { 4 } else { 1 };\n    let stack_layout = GuardedLayout::new(\n        Page::new_user(STACK_GUARD).map_err(|e| ElfProbeError::AddressSpace(e.into()))?,\n        stack_pages,\n    )")
replace("kernel/src/vfs/console.rs",
        "pub fn bootstrap<'a>(\n    root: &'a mut BootstrapRoot,",
        "pub fn bootstrap<'a, const N: usize, const B: usize>(\n    root: &'a mut MemFs<N, B>,")
replace("kernel/src/userspace_io.rs",
        "console::{BootstrapFiles, BootstrapRoot, bootstrap},",
        "console::{BootstrapFiles, bootstrap},\n    memfs::MemFs,")
replace("kernel/src/userspace_io.rs", "struct StaticCell<T>(UnsafeCell<T>);",
        "// Only the static userspace root grows. The legacy console keeps its\n// original small stack-owned filesystem. Capacity remains deterministic.\ntype BootstrapRoot = MemFs<64, 1024>;\n\nstruct StaticCell<T>(UnsafeCell<T>);")
replace("Cargo.toml", "[profile.release]",
        "# Keep the bounded bare-metal shell inside the existing ELF frame pool.\n# Dev overflow/debug checks remain enabled; only optimization changes.\n[profile.dev.package.vibrix-shell]\nopt-level = 2\n\n[profile.dev.package.vibrix-syscall]\nopt-level = 2\n\n[profile.release]")
replace("tools/test_userspace_display.py", "import unittest\n", "import unittest\n\nfrom userspace_cli_cases import check_cli\n")
replace("tools/test_userspace_display.py", 'command("a" * 270, "sh: unknown command")', 'command("a" * 270, "sh: input too long; command not executed")')
replace("tools/test_userspace_display.py", '            command("clear", "\\n" * 32)', '            check_cli(command)\n            command("clear", "\\n" * 32)')
replace("README.md",
        "help    echo    cat    ls    pwd    cd    mkdir    cp    mv    rm    ps    kill    exit",
        "help man apropos which    cat echo ls pwd cd mkdir touch write\ncp mv rm rmdir            head tail wc grep sort uniq nl hexdump\nbasename dirname         ps kill pid vfetch uname clear\ntrue false status history exit")
replace("README.md", "shell pipelines/redirection", "shell pipelines or append redirection")
replace("README.md", "The established PS/2 decoder remains a limited unshifted input subset.", "The PS/2 decoder supports a bounded US-ASCII layout, Shift, Caps Lock, punctuation and Ctrl-U; this is not international or USB keyboard support.")
replace("README.md", "Bootstrap files disappear at reboot.", "Bootstrap files disappear at reboot. The userspace RAM root has 64 slots and a 1024-byte per-file limit. See the [userspace CLI guide](docs/USERSPACE_CLI.md) for quoting, input/output redirection, every command and its limits.")
print("Applied the reviewed CLI integration edits; no syscall number or disk format changed.")
