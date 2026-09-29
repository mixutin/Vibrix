//! Original built-in command catalogue. Help and dispatch share this registry.

pub struct Manual {
    pub command: Builtin,
    pub name: &'static [u8],
    pub summary: &'static [u8],
    pub usage: &'static [u8],
    pub description: &'static [u8],
    pub example: &'static [u8],
}

macro_rules! commands {
    ($($variant:ident, $name:literal, $summary:literal, $usage:literal, $description:literal, $example:literal;)*) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum Builtin { $($variant,)* }

        pub static MANUALS: &[Manual] = &[
            $(Manual {
                command: Builtin::$variant,
                name: $name,
                summary: $summary,
                usage: $usage,
                description: $description,
                example: $example,
            },)*
        ];
    };
}

commands! {
    Help, b"help", b"discover commands and usage", b"help [COMMAND]", b"List built-ins, or show one command's usage. All built-ins accept --help as their only argument, without executing the command.", b"help grep";
    Man, b"man", b"read built-in reference pages", b"man [1] COMMAND | man -k WORD", b"Show NAME, SYNOPSIS, DESCRIPTION, EXAMPLES and limits. Use man shell for syntax. Manuals are embedded, not files or downloaded content.", b"man 1 head";
    Apropos, b"apropos", b"search command names and descriptions", b"apropos WORD", b"Search the command catalogue using an ASCII case-insensitive literal substring. No match returns status 1.", b"apropos file";
    Which, b"which", b"identify shell built-ins", b"which COMMAND...", b"Print the canonical built-in for each name. The alias type does the same. This is not a PATH search; external execution is not implemented.", b"which cat fastfetch";
    Cat, b"cat", b"copy file bytes to output", b"cat [--] [FILE...]", b"Stream files in order. With no FILE, require input redirection. Reads are bounded chunks; no text conversion is performed.", b"cat < /welcome";
    Echo, b"echo", b"print arguments", b"echo [-n] [--] [TEXT...]", b"Join arguments with spaces and normally add a newline. -n omits the newline. Backslash escape sequences such as backslash-n are not interpreted.", b"echo 'hello Vibrix' > /tmp/note";
    Ls, b"ls", b"list directory entries", b"ls [-a] [--] [DIRECTORY]", b"List the current or named directory, marking directories with /. -a includes dot-prefixed names. No fabricated owner, mode, timestamp or disk usage is shown.", b"ls /tmp";
    Pwd, b"pwd", b"print the working directory", b"pwd", b"Print the shell's kernel-validated canonical working directory.", b"pwd";
    Cd, b"cd", b"change working directory", b"cd [DIRECTORY|-]", b"No argument selects /. A dash returns to the previous directory. Failed lookup does not change the working directory. No HOME expansion is performed.", b"cd /tmp";
    Mkdir, b"mkdir", b"create directories", b"mkdir [--] DIRECTORY...", b"Create each named directory. Parent directories must already exist. Existing entries and invalid paths are errors; no recursive creation is implied.", b"mkdir /tmp/work";
    Touch, b"touch", b"create empty files without truncating", b"touch [--] FILE...", b"Create absent files and preserve existing file contents. This bootstrap ABI has no timestamps, so touch does not update access or modification times.", b"touch /tmp/empty";
    Write, b"write", b"replace a file with a line of text", b"write [--] FILE [TEXT...]", b"Create or truncate FILE, then write space-separated arguments and a newline. A failed write is reported; replacement is not atomic. Files remain volatile RAM.", b"write '/tmp/my note' 'hello world'";
    Cp, b"cp", b"copy a regular file", b"cp [--] SOURCE DESTINATION", b"Copy bytes to the exact destination path, creating or truncating it. Same-path aliases are rejected before truncation. No directory recursion or metadata copying. Failed copies may leave a partial destination.", b"cp /welcome /tmp/copy";
    Mv, b"mv", b"copy a file then remove its source", b"mv [--] SOURCE DESTINATION", b"Use the guarded file-copy operation, then remove the source only after successful copying and closing. This is not an atomic rename; a remove failure can leave both files.", b"mv /tmp/copy /tmp/moved";
    Rm, b"rm", b"remove files without recursion", b"rm [--] FILE...", b"Remove named non-directory entries. Directories are refused; use rmdir for an empty directory. There is no recursive or force option.", b"rm /tmp/moved";
    Rmdir, b"rmdir", b"remove empty directories", b"rmdir [--] DIRECTORY...", b"Require directory lookup, then ask the kernel to remove the empty directory. Non-empty, mounted or protected directories remain errors.", b"rmdir /tmp/work";
    Head, b"head", b"print the first lines", b"head [-n COUNT] [--] [FILE]", b"Print the first COUNT lines (default 10). Zero prints nothing. An unterminated final line counts as a line. Without FILE, require input redirection.", b"head -n 1 /welcome";
    Tail, b"tail", b"print the last lines", b"tail [-n COUNT] [--] [FILE]", b"Print the last COUNT lines (default 10), preserving an unterminated final line. Zero prints nothing. No follow mode; input must fit the documented text limit.", b"tail -n 2 /welcome";
    Wc, b"wc", b"count newlines, words and bytes", b"wc [-l|-w|-c] [--] [FILE]", b"Default output is newline, ASCII-whitespace word and byte counts. A selected option prints just that count. This is byte-oriented, not locale-aware character counting.", b"wc -c /welcome";
    Grep, b"grep", b"select lines containing literal text", b"grep [-i] [-n] [-v] [--] PATTERN [FILE]", b"Literal substring matching, not regular expressions. -i ignores ASCII case, -n prefixes line numbers, -v inverts selection. Status is 0 for selected lines, 1 for none and 2 for an error.", b"grep -n RAM /welcome";
    Sort, b"sort", b"sort lines bytewise", b"sort [-r] [-u] [--] [FILE]", b"Sort complete lines in byte order. -r reverses order; -u removes duplicate lines. Output lines end in newline. No numeric, locale or external sorting.", b"sort -u /tmp/names";
    Uniq, b"uniq", b"collapse adjacent duplicate lines", b"uniq [-c] [--] [FILE]", b"Keep one line per adjacent equal run. -c prefixes its count. Non-adjacent duplicates remain. Output lines end in newline; sort first for global deduplication.", b"uniq -c /tmp/names";
    Nl, b"nl", b"number every input line", b"nl [--] [FILE]", b"Prefix all lines, including blank lines, with a one-based number and a tab. This small implementation has no numbering styles or page delimiters.", b"nl /welcome";
    Hexdump, b"hexdump", b"display bytes as hexadecimal", b"hexdump [--] [FILE]", b"Print a hexadecimal offset and up to sixteen hexadecimal bytes per row. Safe for binary input; no format language or terminal control sequences are interpreted.", b"hexdump /welcome";
    Basename, b"basename", b"extract the last path component", b"basename [--] PATH [SUFFIX]", b"Strip trailing slashes, select the final component, and optionally remove a non-empty suffix when it is shorter than the component. Root remains /.", b"basename /tmp/report.txt .txt";
    Dirname, b"dirname", b"extract the parent path", b"dirname [--] PATH", b"Strip the last component without accessing the filesystem. A path with no directory component yields a dot. Root remains /.", b"dirname /tmp/report.txt";
    Ps, b"ps", b"show kernel process records", b"ps", b"Enumerate actual PID, parent PID and running/zombie state through the process-info syscall. This does not imply a multi-user session or job control.", b"ps";
    Kill, b"kill", b"request bootstrap process termination", b"kill PID [STATUS]", b"Call the current kernel kill operation with a numeric PID and non-negative i32 exit status (default 143). This is not POSIX signal-number syntax.", b"kill 2 143";
    Pid, b"pid", b"print the current process ID", b"pid", b"Read the current PID from the kernel; it is not a hard-coded display value.", b"pid";
    Vibrix, b"vibrix", b"show status or run diagnostics", b"vibrix status|doctor", b"Status reports the current userspace/kernel session using real PID/process-table and VFS queries. Doctor runs read-only checks of process visibility, root/dev mounts and bootstrap devices/files. Neither mode claims persistent media, hardware-health, network-link or update-service validation.", b"vibrix doctor";
    Fetch, b"vfetch", b"show native userspace system facts", b"vfetch", b"Render the original Vibrix summary using CPUID, actual privilege bits and kernel PID. neofetch and fastfetch are aliases, not imported packages. Unavailable accounting is labelled unavailable.", b"vfetch";
    Uname, b"uname", b"print OS and architecture identity", b"uname [-a|-s|-m|-r]", b"Default and -s print Vibrix. -m prints x86_64; -r prints the shell build version, not a compatibility promise. -a prints the native Rust userspace identity.", b"uname -a";
    Clear, b"clear", b"clear the native terminal", b"clear", b"Emit one native form-feed control to clear the screen and home the cursor. This uses the existing terminal owner, not an ANSI parser. The control is also mirrored to serial output.", b"clear";
    True, b"true", b"return success", b"true", b"Return status 0 without output. Extra arguments are rejected in this bounded implementation.", b"true";
    False, b"false", b"return failure without a diagnostic", b"false", b"Return status 1 without output. Use status to inspect the result.", b"false";
    Status, b"status", b"print the previous command status", b"status", b"Print the previous command's status, then succeed. Empty input and comments preserve status. No dollar-question-mark expansion is implemented.", b"status";
    History, b"history", b"show recent input lines", b"history", b"Print the last four non-empty accepted input lines, including history itself. Storage is bounded and volatile. No arrow-key recall, history expansion or disk history is implied.", b"history";
    Exit, b"exit", b"end the bootstrap shell session", b"exit [STATUS]", b"Exit using a status from 0 through 255, or the previous status if omitted. The current bootstrap has no shell supervisor; restart the VM for another session.", b"exit 0";
}

impl Builtin {
    pub fn parse(name: &[u8]) -> Option<Self> {
        lookup(name).map(|manual| manual.command)
    }
}

pub fn lookup(name: &[u8]) -> Option<&'static Manual> {
    let canonical = match name {
        b"neofetch" | b"fastfetch" => &b"vfetch"[..],
        b"type" => &b"which"[..],
        other => other,
    };
    MANUALS.iter().find(|manual| manual.name == canonical)
}

pub const SHELL_MANUAL: &[u8] = b"SHELL(1) - Vibrix bounded command language\n\nSYNOPSIS\n  COMMAND [ARGUMENTS] [< INPUT] [> OUTPUT]\n\nDESCRIPTION\n  Single/double quotes group words. Backslash quotes the next byte.\n  Adjacent quoted and unquoted fragments form one argument.\n  Empty quoted arguments are preserved. # starts a comment between words.\n  One input and one output redirection are accepted, even without spaces.\n  Output redirection creates or truncates a file; stderr stays on the TTY.\n  Input/output aliases are rejected before truncation.\n  Use -- to end utility options; quote literal shell punctuation.\n\nEXAMPLES\n  echo 'hello world' > /tmp/note\n  grep hello < /tmp/note\n  man grep\n\nLIMITS\n  254 input bytes, 16 arguments, 255 path bytes.\n  Text filters: 1024 input bytes, at most 128 lines for sort/uniq.\n  No pipes, append, variables, globs, command substitution or job control.\n  All commands are built-ins, not external executable files.\n  RAM files disappear on reboot. No POSIX conformance is claimed.\n\nEXIT STATUS\n  0 success; 1 operation failure/no match; 2 syntax/usage; 127 unknown.\n  grep uses 2 for all errors. See status and each command's manual.\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_command_has_a_complete_manual_and_exact_lookup() {
        for (index, manual) in MANUALS.iter().enumerate() {
            assert_eq!(Builtin::parse(manual.name), Some(manual.command));
            assert!(!manual.summary.is_empty());
            assert!(!manual.usage.is_empty());
            assert!(!manual.description.is_empty());
            assert!(!manual.example.is_empty());
            assert!(
                !MANUALS[..index]
                    .iter()
                    .any(|other| other.name == manual.name)
            );
        }
        assert_eq!(Builtin::parse(b"notacommand"), None);
        assert_eq!(Builtin::parse(b"CAT"), None);
        assert_eq!(Builtin::parse(b"fastfetch"), Some(Builtin::Fetch));
        assert_eq!(Builtin::parse(b"neofetch"), Some(Builtin::Fetch));
        assert_eq!(Builtin::parse(b"type"), Some(Builtin::Which));
    }
}
