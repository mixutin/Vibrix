# Vibrix userspace CLI

Vibrix boots its native Rust shell in Ring 3 through the existing ELF loader, private address space and syscall-backed TTY. This is an original, allocation-free Unix-style command environment, not Linux, BusyBox, a hosted terminal simulation or a POSIX-conformance claim.

The shell provides **38 built-ins and three aliases**, embedded reference pages, quoted arguments, input/output redirection, command status, bounded history, and byte-oriented file/text tools. No additional runtime or third-party package is required.

## Boot and learn

From the repository on the host:

```sh
./tools/run-qemu.sh
```

For the existing loopback VNC launcher:

```sh
./tools/run-qemu.sh --vnc
```

At `vibrix$`, enter one command per line:

```text
help
help grep
grep --help
man grep
man 1 cp
man shell
apropos file
man -k directory
which cat fastfetch
```

`help` is a compact command index. `help COMMAND` and the sole `--help` argument print usage without executing that command, even for `rm`, `kill` and `exit`. This help shortcut also ignores redirections rather than creating or truncating a file. `man` pages contain NAME, SYNOPSIS, DESCRIPTION, EXAMPLES, EXIT STATUS and LIMITS. Descriptions are wrapped for the native terminal. Each manual opens on a fresh screen and fits within its 30 rows; redirected manual output contains no screen-clear control. Pages are embedded in the shell; they do not depend on downloaded content or a `/usr/share/man` filesystem.

`userspace/shell/src/manual.rs` is the single command catalogue used by lookup, help, aliases and manuals. Tests enumerate every entry to prevent undocumented commands or destructive help paths.

## A useful session

```text
mkdir /tmp/demo
cd /tmp/demo
pwd
write 'first note' 'hello world'
echo 'another line' > second
cat 'first note' second > combined
ls
cat < combined
head -n 1 combined
tail -n 1 combined
wc combined
grep -n line combined
sort combined > ordered
uniq -c ordered
nl ordered
hexdump second
cp combined backup
touch backup
mv backup saved
cat saved
false
status
true
status
history
cd -
man shell
```

The first `status` prints `1`; the next prints `0`. `touch backup` preserves its contents. `mv` is deliberately documented as copy-then-remove, not an atomic rename. These files exist only in the guest's volatile RAM filesystem.

## Command reference

Every name below has a built-in `man NAME` page and `NAME --help` path. Options are separate tokens; combined short-option bundles are not supported. Commands accepting `--` use it to end option parsing.

| Command | Synopsis and behavior |
| --- | --- |
| `help` | `help [COMMAND]`: list built-ins or show usage. |
| `man` | `man [1] COMMAND`, `man shell`, or `man -k WORD`: embedded reference or search. |
| `apropos` | `apropos WORD`: ASCII case-insensitive literal name/summary search; no match returns 1. |
| `which` | `which COMMAND...`: identify canonical shell built-ins, not executable files or PATH search. |
| `cat` | `cat [--] [FILE...]`: stream regular-file bytes in order; no FILE requires `< INPUT`. |
| `echo` | `echo [-n] [--] [TEXT...]`: space-separated arguments, optionally without the final newline. |
| `ls` | `ls [-a] [--] [DIRECTORY]`: directory names, with `/` on directories; `-a` includes dot-prefixed names. |
| `pwd` | Print the shell's validated canonical working directory. |
| `cd` | `cd [DIRECTORY|-]`: choose a directory, `/` by default, or the previous directory with `-`. |
| `mkdir` | `mkdir [--] DIRECTORY...`: create directories; parents must exist. |
| `touch` | `touch [--] FILE...`: create absent files without truncating existing files; no timestamps are invented. |
| `write` | `write [--] FILE [TEXT...]`: create/replace a file with a space-separated line. |
| `cp` | `cp [--] SOURCE DESTINATION`: copy a regular file to the exact destination path; no recursive or metadata copy. |
| `mv` | `mv [--] SOURCE DESTINATION`: copy, close successfully, then remove the source; not atomic. |
| `rm` | `rm [--] FILE...`: remove files; refuse directories and recursive deletion. |
| `rmdir` | `rmdir [--] DIRECTORY...`: remove empty directories subject to kernel protection. |
| `head` | `head [-n COUNT] [--] [FILE]`: first lines, default 10. |
| `tail` | `tail [-n COUNT] [--] [FILE]`: last lines, default 10; no follow mode. |
| `wc` | `wc [-l|-w|-c] [--] [FILE]`: newlines, ASCII-whitespace words and bytes, or one selected count. |
| `grep` | `grep [-i] [-n] [-v] [--] PATTERN [FILE]`: literal substring matching, ASCII case folding, line numbers or inversion. |
| `sort` | `sort [-r] [-u] [--] [FILE]`: bytewise line sort, reverse order or unique output. |
| `uniq` | `uniq [-c] [--] [FILE]`: collapse adjacent identical lines, optionally prefixing run counts. |
| `nl` | `nl [--] [FILE]`: number every line, including blank lines, with a tab separator. |
| `hexdump` | `hexdump [--] [FILE]`: hexadecimal offsets and up to 16 bytes per row. |
| `basename` | `basename [--] PATH [SUFFIX]`: lexical final component, optionally stripping a shorter matching suffix. |
| `dirname` | `dirname [--] PATH`: lexical parent component, without filesystem access. |
| `ps` | Actual kernel PID, parent PID and running/zombie state. |
| `kill` | `kill PID [STATUS]`: current bootstrap termination operation; default exit status 143, **not POSIX signal syntax**. |
| `pid` | Current process ID from the kernel. |
| `vibrix` | `vibrix status`: kernel-backed PID/process/VFS overview with explicit persistence/network-status limitations. |
| `vfetch` | Original CPUID, privilege, PID and build summary; unavailable accounting is labelled unavailable. |
| `uname` | `uname [-a|-s|-m|-r]`: identity, architecture or shell build version; `-r` is not a compatibility promise. |
| `clear` | Emit native form feed to clear the existing terminal and home the cursor in one redraw. |
| `true` | Return 0 without output. |
| `false` | Return 1 without an error diagnostic. |
| `status` | Print the previous command's status, then return 0. |
| `history` | Show the last four accepted non-empty input lines, including `history` itself. |
| `exit` | `exit [STATUS]`: end the bootstrap session with 0–255 or the previous status; restart the VM for a new session. |

Aliases: `type` uses the `which` implementation; `neofetch` and `fastfetch` use `vfetch`. No code from those external projects is imported. See [the vfetch and terminal notes](USERSPACE_SHELL.md).

## Shell language

Single and double quotes group words. Empty quoted arguments are retained, adjacent fragments join into one argument, and a backslash quotes the next byte outside single quotes. A `#` at a word boundary starts a comment; `a#b` is one literal word. These are deliberately bounded rules, not the complete POSIX shell expansion grammar.

```text
echo 'a filename with spaces'
echo "another quoted argument"
echo a\ b
echo "" end
echo literal\>character
echo text > /tmp/note
cat</tmp/note
```

One `< INPUT` and one `> OUTPUT` are supported. `>` creates or truncates its target; standard error remains on the terminal. Named inputs and redirected input must be regular files so an unbounded device such as `/dev/zero` cannot monopolize a session that has no job control. Output redirection does not imply descriptor inheritance into external programs.

Parsing completes before opening files. Repeated/append redirection, unclosed quotes, unsupported operators, a missing redirection filename, NUL, too many arguments and saturated input lines are errors rather than partially executed commands. Source/destination aliases are rejected before truncation for copies and input/output redirection. Paths retain kernel lookup validation rather than erasing potentially invalid intermediate components.

A valid command can still fail after opening or writing output. Writes, `cp`, `mv`, and redirection are **not transactional**; an I/O or capacity failure can leave a partial destination. Failed `mv` copies do not remove the source. The shell closes its opened descriptors on both success and error paths.

Exit statuses are 0 for success, 1 for an operation failure or no match, 2 for syntax/usage and 127 for an unknown command. `grep` uses 2 for any error and 1 for no selected lines. Empty input and comments preserve the preceding status. `status` replaces unavailable `$?` expansion.

## Keyboard and resource limits

The QEMU/i8042 decoder supports a bounded US-ASCII layout, separate left/right Shift and Control state, Caps Lock, punctuation, quotes and redirection keys. Backspace edits the canonical line; Ctrl-U clears it. Pause and unsupported extended sequences do not create spurious modifier state. This is not international-layout, Unicode or USB HID keyboard support.

| Resource | Bound |
| --- | --- |
| Accepted command line | 254 bytes; the full 255-byte TTY boundary is refused to avoid executing a discarded suffix. |
| Command arguments | 16 including the command name, plus separately parsed redirection paths. |
| Filesystem path | 255 bytes; backend name/depth constraints also apply. |
| History | Last 4 accepted non-empty lines, volatile, no arrow-key recall. |
| Userspace RAM root | 64 node slots including directories/root; 1024 bytes per file. |
| Text filters | 1024 input bytes; `sort`/`uniq` also require at most 128 lines. |
| Interactive shell stack | 4 guarded user RW/NX pages (16 KiB); other ELF probes keep one page. |

Filters reject oversized input rather than printing a successful truncated result. `cat` and `cp` use streaming chunks instead of the text-filter buffer. The static userspace filesystem grows without enlarging the legacy kernel console's stack-owned `MemFs<16, 256>`. The existing 32-frame private-address-space pool, syscall ABI, disk formats, device owner and guard-page protections are unchanged. Package-specific development optimization keeps the shell image bounded while retaining inherited debug/overflow checks.

## Not implemented

Files disappear on reboot. There is no persistent USB root in this CLI change. It does not add external executable search/launch, `fork`/`exec`, pipelines, `>>`, variables, glob expansion, command substitution, scripts, job control, full POSIX signals, users/permissions tooling, package management, networking commands, a pager, or full ANSI/VT emulation. `grep` is literal, `wc` is byte-oriented, and `nl` numbers all lines; familiar names do not promise every Unix option.

Persistent storage and a process/descriptor lifecycle supporting external programs are separate kernel integration work, not features simulated by these commands. No internal disk is installed to or formatted.

## Verification and implementation references

`cargo test --locked -p vibrix-shell --lib` exercises the production parser, command catalogue, text algorithms and dispatcher through a host `System` fixture. This includes all help/manual paths, quoted names, same-file guards, non-truncating touch, directory errors, status semantics, partial/zero writes, oversized reads, file/filter limits and descriptor cleanup.

`tools/test_userspace_display.py` calls `tools/userspace_cli_cases.py` inside the actual QEMU/RFB session. The cases type quotes and redirection through the real decoder, create/copy/filter files, inspect command status, reject same-file operations, exceed the previous 256-byte file and 16-node limits, and then retain the existing independent clear/scroll pixel assertions. Workflow logs and `display-result.json` identify the exact source tested; a host-test pass alone is not boot evidence.

The implementation is original Rust, developed by **GPT-6 Astra Pro**, with no new dependency. Design references checked on 2026-09-29:

- [POSIX.1-2024 shell language](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html), used to distinguish familiar syntax from deliberately unsupported expansion and process semantics, not as copied implementation code.
- [Rust core slice documentation](https://doc.rust-lang.org/core/primitive.slice.html), for allocation-free byte and slice operations.
- [Cargo profile overrides](https://doc.rust-lang.org/cargo/reference/profiles.html), for package-specific optimization without changing global safety checks.
- [Microsoft keyboard input and Scan 1 make-code table](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-keyboard-input), for the bounded original US-ASCII decoder.
- Repository-owned syscall ABI, VFS, guarded address-space loader, TTY, framebuffer terminal and RFB/QMP pixel harness. No competing implementation of those owners was introduced.
